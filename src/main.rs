// Copyright 2019 Hristo Venev
//
// See COPYING.

#[macro_use]
extern crate arrayref;

use std::io;
use std::time::{Duration, Instant, SystemTime};

mod bin;
mod builder;
mod config;
mod ip;
mod model;
mod proto;
mod wg;

struct Source {
    config: config::Source,
    data: Option<proto::Source>,
    next_update: Instant,
    backoff: Option<Duration>,
}

impl Source {
    #[inline]
    fn new(config: config::Source) -> Source {
        Source {
            config,
            data: None,
            next_update: Instant::now(),
            backoff: None,
        }
    }
}

pub struct Device {
    dev: wg::Device,
    peer_config: config::PeerConfig,
    update_config: config::UpdateConfig,
    sources: Vec<Source>,
    current: model::Config,
}

impl Device {
    pub fn new(ifname: String, c: config::Config) -> io::Result<Device> {
        let dev = wg::Device::new(ifname)?;

        Ok(Device {
            dev,
            peer_config: c.peer_config,
            update_config: c.update_config,
            sources: c.sources.into_iter().map(Source::new).collect(),
            current: model::Config::default(),
        })
    }

    fn make_config(
        &self,
        public_key: model::Key,
        ts: SystemTime,
    ) -> (model::Config, Vec<builder::ConfigError>, SystemTime) {
        let mut t_cfg = ts + Duration::from_secs(1 << 30);
        let mut sources: Vec<(&Source, &proto::SourceConfig)> = vec![];
        for src in self.sources.iter() {
            if let Some(ref data) = src.data {
                let sc = data
                    .next
                    .as_ref()
                    .and_then(|next| {
                        if ts >= next.update_at {
                            Some(&next.config)
                        } else {
                            t_cfg = t_cfg.min(next.update_at);
                            None
                        }
                    })
                    .unwrap_or(&data.config);
                sources.push((src, sc));
            }
        }

        let mut cfg = builder::ConfigBuilder::new(public_key, &self.peer_config);

        for (src, sc) in sources.iter() {
            for peer in sc.servers.iter() {
                cfg.add_server(&src.config, peer);
            }
        }

        for (src, sc) in sources.iter() {
            for peer in sc.road_warriors.iter() {
                cfg.add_road_warrior(&src.config, peer);
            }
        }

        let (cfg, errs) = cfg.build();
        (cfg, errs, t_cfg)
    }

    pub fn update(&mut self) -> io::Result<Instant> {
        let refresh = Duration::from_secs(u64::from(self.update_config.refresh_sec));
        let mut now = Instant::now();
        let mut t_refresh = now + refresh;

        for src in self.sources.iter_mut() {
            if now < src.next_update {
                t_refresh = t_refresh.min(src.next_update);
                continue;
            }

            let r = fetch_source(&src.config.url);
            now = Instant::now();
            let r = match r {
                Ok(r) => {
                    eprintln!("<6>Updated [{}]", &src.config.url);
                    src.data = Some(r);
                    src.backoff = None;
                    src.next_update = now + refresh;
                    continue;
                }
                Err(r) => r,
            };

            let b = src.backoff.unwrap_or(if src.data.is_some() {
                refresh / 3
            } else {
                Duration::from_secs(10).min(refresh / 10)
            });
            src.next_update = now + b;
            t_refresh = t_refresh.min(src.next_update);

            eprintln!("<3>Failed to update [{}], retrying after {:?}: {}", &src.config.url, b, &r);

            src.backoff = Some((b + b / 3).min(refresh));
        }

        let now = Instant::now();
        let sysnow = SystemTime::now();
        let public_key = self.dev.get_public_key()?;
        let (config, errors, t_cfg) = self.make_config(public_key, sysnow);
        let time_to_cfg = t_cfg
            .duration_since(sysnow)
            .unwrap_or(Duration::from_secs(0));
        let t_cfg = now + time_to_cfg;

        if config != self.current {
            eprintln!("<5>Applying configuration update");
            for err in errors.iter() {
                eprintln!("<{}>{}", if err.important { '4' } else { '5' }, err);
            }
            self.dev.apply_diff(&self.current, &config)?;
            self.current = config;
        }

        Ok(if t_cfg < t_refresh {
            eprintln!("<6>Next configuration update after {:?}", time_to_cfg);
            t_cfg
        } else if t_refresh > now {
            t_refresh
        } else {
            eprintln!("<4>Next refresh immediately?");
            now
        })
    }
}

fn fetch_source(url: &str) -> io::Result<proto::Source> {
    use std::env;
    use std::ffi::{OsStr, OsString};
    use std::process::{Command, Stdio};

    let curl = match env::var_os("CURL") {
        None => OsString::new(),
        Some(v) => v,
    };
    let mut proc = Command::new(if curl.is_empty() {
        OsStr::new("curl")
    } else {
        curl.as_os_str()
    });

    proc.stdin(Stdio::null());
    proc.stdout(Stdio::piped());
    proc.stderr(Stdio::piped());
    proc.arg("-gsSfL");
    proc.arg("--fail-early");
    proc.arg("--max-time");
    proc.arg("10");
    proc.arg("--max-filesize");
    proc.arg("1M");
    proc.arg("--");
    proc.arg(url);

    let out = proc.output()?;

    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stderr);
        let msg = msg.replace('\n', "; ");
        return Err(io::Error::new(io::ErrorKind::Other, msg));
    }

    let mut de = serde_json::Deserializer::from_slice(&out.stdout);
    let r = serde::Deserialize::deserialize(&mut de)?;
    Ok(r)
}

fn load_config(path: &str) -> io::Result<config::Config> {
    use std::fs;
    use toml;

    let mut data = String::new();
    {
        use io::Read;
        let mut config_file = fs::File::open(path)?;
        config_file.read_to_string(&mut data)?;
    }
    let mut de = toml::Deserializer::new(&data);
    serde::Deserialize::deserialize(&mut de)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn main() {
    use std::{env, process, thread};

    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        let arg0 = if !args.is_empty() { &args[0] } else { "wgconf" };
        eprintln!("<1>Usage:");
        eprintln!("<1>    {} IFNAME CONFIG", arg0);
        process::exit(1);
    }

    let mut args = args.into_iter();
    let _ = args.next().unwrap();
    let ifname = args.next().unwrap();
    let config_path = args.next().unwrap();
    assert!(args.next().is_none());

    let config = match load_config(&config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("<1>Failed to load config: {}", e);
            process::exit(1);
        }
    };

    let mut dev = match Device::new(ifname, config) {
        Ok(dev) => dev,
        Err(e) => {
            eprintln!("<1>Failed to open device: {}", e);
            process::exit(1);
        }
    };

    loop {
        let tm = match dev.update() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("<1>{}", e);
                process::exit(1);
            }
        };
        let now = Instant::now();
        if tm > now {
            let sleep = tm.duration_since(now);
            thread::sleep(sleep);
        }
    }
}
