// Copyright 2019 Hristo Venev
//
// See COPYING.

#[macro_use]
extern crate arrayref;

use std::io;
use std::time::{Duration, Instant, SystemTime};

mod bin;
mod config;
mod ip;
mod proto;
mod wg;

struct Source {
    config: config::Source,
    data: Option<proto::Source>,
    next_update: Instant,
    backoff: Option<Duration>,
}

impl Source {
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
    current: wg::Config,
}

impl Device {
    pub fn new(ifname: String, c: config::Config) -> io::Result<Device> {
        let dev = wg::Device::new(ifname)?;

        Ok(Device {
            dev,
            peer_config: c.peer_config,
            update_config: c.update_config,
            sources: c.sources.into_iter().map(Source::new).collect(),
            current: wg::Config::default(),
        })
    }

    fn refresh_period(&self) -> Duration {
        Duration::from_secs(u64::from(self.update_config.refresh_period))
    }

    fn make_config(
        &self,
        public_key: &str,
        ts: SystemTime,
    ) -> (wg::Config, Vec<wg::ConfigError>, SystemTime) {
        let mut t_cfg = ts + self.refresh_period();
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

        let mut cfg = wg::ConfigBuilder::new(public_key, &self.peer_config);
        let mut errs = vec![];
        for (src, sc) in sources.iter() {
            for peer in sc.servers.iter() {
                cfg.add_server(&mut errs, &src.config, peer);
            }
        }
        for (src, sc) in sources.iter() {
            for peer in sc.road_warriors.iter() {
                cfg.add_road_warrior(&mut errs, &src.config, peer);
            }
        }

        let cfg = cfg.build();
        (cfg, errs, t_cfg)
    }

    pub fn update(&mut self) -> io::Result<Instant> {
        let refresh = self.refresh_period();
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

            eprintln!("<3>Failed to update [{}]: {}", &src.config.url, &r);

            let b = src.backoff.unwrap_or(if src.data.is_some() {
                refresh / 3
            } else {
                Duration::from_secs(10).min(refresh / 10)
            });
            src.next_update = now + b;
            t_refresh = t_refresh.min(src.next_update);
            let b = (b + b / 3).min(refresh);
            src.backoff = Some(b);
        }

        let now = Instant::now();
        let sysnow = SystemTime::now();
        let public_key = self.dev.get_public_key()?;
        let (config, errors, t_cfg) = self.make_config(&public_key, sysnow);
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
            eprintln!("<6>Next refresh after {:?}", t_refresh.duration_since(now));
            t_refresh
        } else {
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
    proc.stderr(Stdio::null());
    proc.arg("--fail");
    proc.arg("--fail-early");
    proc.arg("--");
    proc.arg(url);

    let out = proc.output()?;

    if !out.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to download [{}]", url),
        ));
    }

    let mut de = serde_json::Deserializer::from_slice(&out.stdout);
    let r = serde::Deserialize::deserialize(&mut de)?;
    Ok(r)
}

fn load_config(path: &str) -> io::Result<config::Config> {
    use serde_json;
    use std::fs;

    let config_file = fs::File::open(path)?;
    let rd = io::BufReader::new(config_file);
    let mut de = serde_json::Deserializer::from_reader(rd);
    Ok(serde::Deserialize::deserialize(&mut de)?)
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
