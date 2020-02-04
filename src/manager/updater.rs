// SPDX-License-Identifier: LGPL-3.0-or-later
//
// Copyright 2019 Hristo Venev

use super::Source;
use crate::{config, fileutil, proto};
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use std::{fs, io};

pub(super) struct Updater {
    config: config::UpdaterConfig,
}

impl Updater {
    pub fn new(config: config::UpdaterConfig) -> Self {
        Self { config }
    }

    fn cache_path(&self, s: &Source) -> Option<PathBuf> {
        let mut p = self.config.cache_directory.as_ref()?.clone();
        p.push(&s.config.name);
        Some(p)
    }

    fn cache_update(&self, src: &Source) {
        let path = match self.cache_path(src) {
            Some(v) => v,
            None => return,
        };

        let data = serde_json::to_vec(&src.data).unwrap();
        match fileutil::update(&path, &data) {
            Ok(()) => {}
            Err(e) => {
                eprintln!("<4>Failed to cache [{}]: {}", &src.config.name, e);
            }
        }
    }

    pub fn cache_load(&self, src: &mut Source) -> bool {
        let path = match self.cache_path(src) {
            Some(v) => v,
            None => return false,
        };

        let data = match fileutil::load(&path) {
            Ok(Some(data)) => data,
            Ok(None) => {
                return false;
            }
            Err(e) => {
                eprintln!("<3>Failed to read [{}] from cache: {}", &src.config.name, e);
                return false;
            }
        };

        let mut de = serde_json::Deserializer::from_slice(&data);
        src.data = match serde::Deserialize::deserialize(&mut de) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("<3>Failed to load [{}] from cache: {}", &src.config.name, e);
                return false;
            }
        };

        true
    }

    pub fn update(&self, src: &mut Source) -> (bool, Instant) {
        let refresh = self.refresh_time();

        let r = fetch_source(&src.config.url);
        let now = Instant::now();
        let r = match r {
            Ok(r) => {
                eprintln!("<6>Updated [{}]", &src.config.url);
                src.data = r;
                src.backoff = None;
                src.next_update = now + refresh;
                self.cache_update(src);
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

    pub fn refresh_time(&self) -> Duration {
        Duration::from_secs(u64::from(self.config.refresh_sec))
    }
}

fn fetch_source(url: &str) -> io::Result<proto::Source> {
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
