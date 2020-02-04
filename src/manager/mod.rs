// SPDX-License-Identifier: LGPL-3.0-or-later
//
// Copyright 2019 Hristo Venev

use crate::{config, fileutil, model, proto, wg};
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

struct Source {
    name: String,
    config: config::Source,
    data: proto::Source,
    next_update: Instant,
    backoff: Option<Duration>,
}

mod updater;
pub use updater::load_source;

mod builder;

pub struct Manager {
    dev: wg::Device,
    global_config: config::GlobalConfig,
    sources: Vec<Source>,
    current: model::Config,
    state_path: PathBuf,
    updater: updater::Updater,
}

impl Manager {
    pub fn new(ifname: OsString, c: config::Config) -> io::Result<Self> {
        let runtime_directory = c.runtime_directory.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "runtime directory required")
        })?;

        let mut state_path = runtime_directory;
        state_path.push("state.json");

        let mut m = Self {
            dev: wg::Device::open(ifname)?,
            global_config: c.global,
            sources: vec![],
            current: model::Config::empty(),
            state_path,
            updater: updater::Updater::new(c.updater),
        };

        let _ = m.current_load();

        for (name, cfg) in c.sources {
            m.add_source(name, cfg)?;
        }

        Ok(m)
    }

    fn current_load(&mut self) -> bool {
        let data = match fileutil::load(&self.state_path) {
            Ok(Some(data)) => data,
            Ok(None) => {
                return false;
            }
            Err(e) => {
                eprintln!("<3>Failed to read interface state: {}", e);
                return false;
            }
        };

        let mut de = serde_json::Deserializer::from_slice(&data);
        match serde::Deserialize::deserialize(&mut de) {
            Ok(c) => {
                self.current = c;
                true
            }
            Err(e) => {
                eprintln!("<3>Failed to load interface state: {}", e);
                false
            }
        }
    }

    fn current_update(&mut self, c: &model::Config) {
        let data = serde_json::to_vec(c).unwrap();
        match fileutil::update(&self.state_path, &data) {
            Ok(()) => {}
            Err(e) => {
                eprintln!("<3>Failed to persist interface state: {}", e);
            }
        }
    }

    fn add_source(&mut self, name: String, config: config::Source) -> io::Result<()> {
        let mut s = Source {
            name,
            config,
            data: proto::Source::empty(),
            next_update: Instant::now(),
            backoff: None,
        };

        self.init_source(&mut s)?;
        self.sources.push(s);
        Ok(())
    }

    fn init_source(&mut self, s: &mut Source) -> io::Result<()> {
        if self.updater.update(s).0 {
            return Ok(());
        }
        if self.updater.cache_load(s) {
            return Ok(());
        }
        if !s.config.required {
            return Ok(());
        }
        if self.updater.update(s).0 {
            return Ok(());
        }
        if self.updater.update(s).0 {
            return Ok(());
        }
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("failed to update required source [{}]", &s.config.url),
        ))
    }

    fn make_config(
        &self,
        public_key: model::Key,
        ts: SystemTime,
    ) -> (model::Config, Vec<builder::Error>, SystemTime) {
        let mut t_cfg = ts + Duration::from_secs(1 << 20);
        let mut sources: Vec<(&Source, &proto::SourceConfig)> = vec![];
        for src in &self.sources {
            let sc = src
                .data
                .next
                .as_ref()
                .and_then(|next| {
                    if ts >= next.0 {
                        Some(&next.1)
                    } else {
                        t_cfg = t_cfg.min(next.0);
                        None
                    }
                })
                .unwrap_or(&src.data.config);
            sources.push((src, sc));
        }

        let mut cfg = builder::ConfigBuilder::new(public_key, &self.global_config);

        for (src, sc) in &sources {
            for peer in &sc.servers {
                cfg.add_server(src, peer);
            }
        }

        for (src, sc) in &sources {
            for peer in &sc.road_warriors {
                cfg.add_road_warrior(src, peer);
            }
        }

        let (cfg, errs) = cfg.build();
        (cfg, errs, t_cfg)
    }

    fn refresh(&mut self) -> io::Result<Instant> {
        let refresh = self.updater.refresh_time();
        let mut now = Instant::now();
        let mut t_refresh = now + refresh;

        for src in &mut self.sources {
            if now >= src.next_update {
                now = self.updater.update(src).1;
            }
            t_refresh = t_refresh.min(src.next_update);
        }

        Ok(t_refresh)
    }

    pub fn update(&mut self) -> io::Result<Instant> {
        let t_refresh = self.refresh()?;

        let public_key = self.dev.get_public_key()?;
        let now = Instant::now();
        let sysnow = SystemTime::now();
        let (config, errors, t_cfg) = self.make_config(public_key, sysnow);
        let time_to_cfg = t_cfg
            .duration_since(sysnow)
            .unwrap_or(Duration::from_secs(0));
        let t_cfg = now + time_to_cfg;

        if config != self.current {
            if errors.is_empty() {
                eprintln!("<5>Applying configuration update");
            } else {
                eprint!(
                    "<{}>New update contains errors: ",
                    if errors.iter().any(|err| err.important()) {
                        '4'
                    } else {
                        '5'
                    }
                );
                for err in &errors {
                    eprint!("{}; ", err);
                }
                eprintln!("applying anyway");
            }
            self.dev.apply_diff(&self.current, &config)?;
            self.current_update(&config);
            self.current = config;
        }

        Ok(if t_cfg < t_refresh {
            eprintln!("<6>Next configuration update after {:.1?}", time_to_cfg);
            t_cfg
        } else if t_refresh > now {
            t_refresh
        } else {
            eprintln!("<4>Next refresh immediately?");
            now
        })
    }
}
