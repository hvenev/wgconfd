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
    backoff: Option<u32>,
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
    pub fn new(c: config::Config) -> Device {
        Device {
            dev: wg::Device::new(c.ifname, c.wg_command),
            peer_config: c.peers,
            update_config: c.update,
            sources: c.sources.into_iter().map(Source::new).collect(),
            current: wg::Config::new(),
        }
    }

    fn make_config(&self, ts: SystemTime) -> (wg::Config, Vec<wg::ConfigError>, SystemTime) {
        let mut cfg = wg::Config::new();
        let mut next_update = ts + Duration::from_secs(3600);
        let mut errs = vec![];
        for src in self.sources.iter() {
            if let Some(data) = &src.data {
                let sc = data
                    .next
                    .as_ref()
                    .and_then(|next| {
                        if ts >= next.update_at {
                            Some(&next.config)
                        } else {
                            next_update = next_update.min(next.update_at);
                            None
                        }
                    })
                    .unwrap_or(&data.config);
                for peer in sc.peers.iter() {
                    cfg.add_peer(&mut errs, &self.peer_config, &src.config, peer);
                }
            }
        }
        (cfg, errs, next_update)
    }

    pub fn update(&mut self) -> io::Result<Instant> {
        let now = Instant::now();
        let refresh = self.update_config.refresh_period;
        let after_refresh = now + Duration::from_secs(u64::from(refresh));
        let mut next_update = after_refresh;
        for src in self.sources.iter_mut() {
            if now < src.next_update {
                next_update = next_update.min(src.next_update);
                continue;
            }

            let r = fetch_source(&src.config.url);
            let r = match r {
                Ok(r) => {
                    eprintln!("<6>Updated [{}]", &src.config.url);
                    src.data = Some(r);
                    src.backoff = None;
                    src.next_update = after_refresh;
                    continue;
                }
                Err(r) => r,
            };

            eprintln!("<3>Failed to update [{}]: {}", &src.config.url, &r);

            let b = src.backoff.unwrap_or(if src.data.is_some() {
                refresh / 3
            } else {
                u32::min(10, refresh / 10)
            });
            let b = (b + b / 3).min(refresh);
            src.backoff = Some(b);
            src.next_update = now + Duration::from_secs(u64::from(b));
            next_update = next_update.min(src.next_update);
        }

        let sysnow = SystemTime::now();
        let (config, errors, upd_time) = self.make_config(sysnow);
        let time_to_upd = upd_time
            .duration_since(sysnow)
            .unwrap_or(Duration::from_secs(0));
        next_update = next_update.min(now + time_to_upd);

        if config != self.current {
            eprintln!("<5>Applying configuration update");
            for err in errors.iter() {
                eprintln!("<{}>{}", if err.important { '4' } else { '5' }, err);
            }
            self.dev.apply_diff(&self.current, &config)?;
            self.current = config;
        }
        eprintln!("<6>Next configuration update after {:?}", time_to_upd);

        Ok(next_update)
    }
}

fn fetch_source(url: &str) -> io::Result<proto::Source> {
    use curl::easy::Easy;

    let mut res = Vec::<u8>::new();

    {
        let mut req = Easy::new();
        req.url(url)?;

        {
            let mut tr = req.transfer();
            tr.write_function(|data| {
                res.extend_from_slice(data);
                Ok(data.len())
            })?;
            tr.perform()?;
        }

        let code = req.response_code()?;
        if code != 0 && code != 200 {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("HTTP error {}", code),
            ));
        }
    }

    let mut de = serde_json::Deserializer::from_slice(&res);
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
    if args.len() != 2 {
        let arg0 = if !args.is_empty() { &args[0] } else { "wgconf" };
        eprintln!("<1>Usage:");
        eprintln!("<1>    {} CONFIG", arg0);
        process::exit(1);
    }

    let config = match load_config(&args[1]) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("<1>Failed to load config: {}", e);
            process::exit(1);
        }
    };

    let mut dev = Device::new(config);
    loop {
        let tm = match dev.update() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("<1>{}", e);
                process::exit(1);
            }
        };
        let now = Instant::now();
        let sleep = tm.duration_since(now);
        println!("Sleeping for {:?}", sleep);
        thread::sleep(sleep);
    }
}
