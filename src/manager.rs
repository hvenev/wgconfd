// Copyright 2019 Hristo Venev
//
// See COPYING.

use crate::{builder, config, model, proto, wg};
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};
use std::{fs, io};

struct Source {
    name: String,
    config: config::Source,
    data: proto::Source,
    next_update: Instant,
    backoff: Option<Duration>,
}

struct Updater {
    config: config::UpdateConfig,
    cache_directory: Option<PathBuf>,
}

impl Updater {
    fn cache_path(&self, s: &Source) -> Option<PathBuf> {
        if let Some(ref dir) = self.cache_directory {
            let mut p = dir.clone();
            p.push(&s.name);
            Some(p)
        } else {
            None
        }
    }

    fn cache_update(&self, src: &Source) -> io::Result<bool> {
        let path = if let Some(path) = match self.cache_path(src) {
            path
        } else {
            return Ok(false);
        };

        let mut tmp_path = OsString::from(path.clone());
        tmp_path.push(".tmp");
        let tmp_path = PathBuf::from(tmp_path);

        let data = serde_json::to_vec(&src.data).unwrap();

        let mut file = fs::File::create(&tmp_path)?;
        match io::Write::write_all(&mut file, &data)
            .and_then(|_| file.sync_data())
            .and_then(|_| fs::rename(&tmp_path, &path))
        {
            Ok(()) => {}
            Err(e) => {
                fs::remove_file(&tmp_path).unwrap_or_else(|e2| {
                    eprintln!("<3>Failed to clean up [{}]: {}", tmp_path.display(), e2);
                });
                return Err(e);
            }
        }

        Ok(true)
    }

    fn cache_load(&self, src: &mut Source) -> bool {
        let path = if let Some(path) = match self.cache_path(src) {
            path
        } else {
            return false;
        };

        let mut file = if let Some(file) = fs::File::open(&path) {
            file
        } else {
            return false;
        };

        let mut data = Vec::new();
        match io::Read::read_to_end(&mut file, &mut data) {
            Ok(_) => {}
            Err(e) => {
                eprintln!("<3>Failed to read [{}] from cache: {}", src.config.url, e);
                return false;
            }
        };

        let mut de = serde_json::Deserializer::from_slice(&data);
        src.data = match serde::Deserialize::deserialize(&mut de) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("<3>Failed to load [{}] from cache: {}", src.config.url, e);
                return false;
            }
        };

        true
    }

    fn update(&self, src: &mut Source) -> (bool, Instant) {
        let refresh = Duration::from_secs(u64::from(self.config.refresh_sec));

        let r = fetch_source(&src.config.url);
        let now = Instant::now();
        let r = match r {
            Ok(r) => {
                eprintln!("<6>Updated [{}]", &src.config.url);
                src.data = r;
                src.backoff = None;
                src.next_update = now + refresh;
                match self.cache_update(src) {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("<4>Failed to cache [{}]: {}", &src.config.url, e);
                    }
                }
                return (true, now);
            }
            Err(r) => r,
        };

        let b = src
            .backoff
            .unwrap_or_else(|| Duration::from_secs(10).min(refresh / 10));
        src.next_update = now + b;
        src.backoff = Some((b + b / 3).min(refresh / 3));
        eprintln!(
            "<3>Failed to update [{}], retrying after {:.1?}: {}",
            &src.config.url, b, &r
        );
        (false, now)
    }
}

pub struct Manager {
    dev: wg::Device,
    peer_config: config::PeerConfig,
    sources: Vec<Source>,
    current: model::Config,
    updater: Updater,
}

impl Manager {
    pub fn new(ifname: OsString, c: config::Config) -> io::Result<Self> {
        let mut m = Self {
            dev: wg::Device::new(ifname)?,
            peer_config: c.peer_config,
            sources: vec![],
            current: model::Config::default(),
            updater: Updater {
                config: c.update_config,
                cache_directory: c.cache_directory,
            },
        };

        for (name, cfg) in c.sources {
            m.add_source(name, cfg)?;
        }

        Ok(m)
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
    ) -> (model::Config, Vec<builder::ConfigError>, SystemTime) {
        let mut t_cfg = ts + Duration::from_secs(1 << 30);
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

        let mut cfg = builder::ConfigBuilder::new(public_key, &self.peer_config);

        for (src, sc) in &sources {
            for peer in &sc.servers {
                cfg.add_server(&src.config, peer);
            }
        }

        for (src, sc) in &sources {
            for peer in &sc.road_warriors {
                cfg.add_road_warrior(&src.config, peer);
            }
        }

        let (cfg, errs) = cfg.build();
        (cfg, errs, t_cfg)
    }

    fn refresh(&mut self) -> io::Result<Instant> {
        let refresh = Duration::from_secs(u64::from(self.updater.config.refresh_sec));
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
                eprintln!("<{}>{}", if err.important { '4' } else { '5' }, err);
            }
            self.dev.apply_diff(&self.current, &config)?;
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

pub fn fetch_source(url: &str) -> io::Result<proto::Source> {
    use std::env;
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

pub fn load_source(path: &OsStr) -> io::Result<proto::Source> {
    let mut data = Vec::new();
    {
        use std::io::Read;
        let mut f = fs::File::open(&path)?;
        f.read_to_end(&mut data)?;
    }

    let mut de = serde_json::Deserializer::from_slice(&data);
    let r = serde::Deserialize::deserialize(&mut de)?;
    Ok(r)
}
