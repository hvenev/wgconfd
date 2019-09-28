// Copyright 2019 Hristo Venev
//
// See COPYING.

use crate::{config, model, proto, wg};
use std::ffi::OsString;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};
use std::{fs, io};

fn update_file(path: &Path, data: &[u8]) -> io::Result<()> {
    let mut tmp_path = OsString::from(path);
    tmp_path.push(".tmp");
    let tmp_path = PathBuf::from(tmp_path);

    let mut file = {
        let mut file = fs::OpenOptions::new();
        file.append(true);
        file.create_new(true);
        #[cfg(unix)]
        file.mode(0o0600);
        file.open(&tmp_path)?
    };

    let r = io::Write::write_all(&mut file, data)
        .and_then(|_| file.sync_data())
        .and_then(|_| fs::rename(&tmp_path, &path));

    if r.is_err() {
        fs::remove_file(&tmp_path).unwrap_or_else(|e2| {
            eprintln!("<3>Failed to clean up [{}]: {}", tmp_path.display(), e2);
        });
    }
    r
}

fn load_file(path: &Path) -> io::Result<Option<Vec<u8>>> {
    let mut file = match fs::File::open(&path) {
        Ok(file) => file,
        Err(e) => {
            if e.kind() == io::ErrorKind::NotFound {
                return Ok(None);
            }
            return Err(e);
        }
    };

    let mut data = Vec::new();
    io::Read::read_to_end(&mut file, &mut data)?;
    Ok(Some(data))
}

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
    state_directory: Option<PathBuf>,
    updater: updater::Updater,
}

impl Manager {
    pub fn new(ifname: OsString, c: config::Config) -> io::Result<Self> {
        let mut m = Self {
            dev: wg::Device::open(ifname)?,
            global_config: c.global,
            sources: vec![],
            current: model::Config::empty(),
            state_directory: c.state_directory,
            updater: updater::Updater::new(c.updater),
        };

        let _ = m.current_load();

        for (name, cfg) in c.sources {
            m.add_source(name, cfg)?;
        }

        Ok(m)
    }

    fn state_path(&self) -> Option<PathBuf> {
        let mut path = self.state_directory.as_ref()?.clone();
        path.push("state.json");
        Some(path)
    }

    fn current_load(&mut self) -> bool {
        let path = match self.state_path() {
            Some(v) => v,
            None => return false,
        };

        let data = match load_file(&path) {
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
        let path = match self.state_path() {
            Some(v) => v,
            None => return,
        };

        let data = serde_json::to_vec(c).unwrap();
        match update_file(&path, &data) {
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
            format!("Failed to update required source [{}]", &s.config.url),
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
                    if ts >= next.update_at {
                        Some(&next.config)
                    } else {
                        t_cfg = t_cfg.min(next.update_at);
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
            eprintln!("<5>Applying configuration update");
            for err in &errors {
                eprintln!("<{}>{}", if err.important() { '4' } else { '5' }, err);
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
