// SPDX-License-Identifier: LGPL-3.0-or-later
//
// Copyright 2019,2020 Hristo Venev

use crate::model::{Endpoint, Ipv4Set, Ipv6Set, Key, Secret};
use serde_derive;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(serde_derive::Serialize, serde_derive::Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub url: String,
    pub psk: Option<Secret>,
    pub ipv4: Ipv4Set,
    pub ipv6: Ipv6Set,
    #[serde(default)]
    pub required: bool,
}

#[derive(serde_derive::Serialize, serde_derive::Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(deny_unknown_fields)]
pub struct Peer {
    pub source: Option<String>,
    pub endpoint: Option<Endpoint>,
    pub psk: Option<Secret>,
    pub keepalive: Option<u32>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GlobalConfig {
    pub min_keepalive: u32,
    pub max_keepalive: u32,
    pub peers: HashMap<Key, Peer>,
}

impl Default for GlobalConfig {
    #[inline]
    fn default() -> Self {
        Self {
            min_keepalive: default_min_keepalive(),
            max_keepalive: default_max_keepalive(),
            peers: HashMap::new(),
        }
    }
}

impl GlobalConfig {
    pub fn fix_keepalive(&self, mut k: u32) -> u32 {
        if self.max_keepalive != 0 && (k == 0 || k > self.max_keepalive) {
            k = self.max_keepalive;
        }
        if k != 0 && k < self.min_keepalive {
            k = self.min_keepalive;
        }
        k
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct UpdaterConfig {
    pub cache_directory: Option<PathBuf>,

    // Number of seconds between regular updates.
    pub refresh_sec: u32,
}

impl Default for UpdaterConfig {
    #[inline]
    fn default() -> Self {
        Self {
            cache_directory: None,
            refresh_sec: default_refresh_sec(),
        }
    }
}

#[derive(serde_derive::Serialize, serde_derive::Deserialize, Default, Clone, Debug)]
#[serde(from = "ConfigRepr", into = "ConfigRepr")]
pub struct Config {
    pub runtime_directory: Option<PathBuf>,
    pub global: GlobalConfig,
    pub updater: UpdaterConfig,
    pub sources: HashMap<String, Source>,
}

#[derive(serde_derive::Serialize, serde_derive::Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigRepr {
    runtime_directory: Option<PathBuf>,
    cache_directory: Option<PathBuf>,

    #[serde(default = "default_min_keepalive")]
    min_keepalive: u32,
    #[serde(default = "default_max_keepalive")]
    max_keepalive: u32,
    #[serde(default, rename = "peer")]
    peers: HashMap<Key, Peer>,

    #[serde(default = "default_refresh_sec")]
    refresh_sec: u32,

    #[serde(default, rename = "source")]
    sources: HashMap<String, Source>,
}

impl From<Config> for ConfigRepr {
    #[inline]
    fn from(v: Config) -> Self {
        let Config {
            runtime_directory,
            global,
            updater,
            sources,
        } = v;
        let GlobalConfig {
            min_keepalive,
            max_keepalive,
            peers,
        } = global;
        let UpdaterConfig {
            cache_directory,
            refresh_sec,
        } = updater;
        Self {
            runtime_directory,
            cache_directory,
            min_keepalive,
            max_keepalive,
            peers,
            refresh_sec,
            sources,
        }
    }
}

impl From<ConfigRepr> for Config {
    #[inline]
    fn from(v: ConfigRepr) -> Self {
        let ConfigRepr {
            runtime_directory,
            cache_directory,
            min_keepalive,
            max_keepalive,
            peers,
            refresh_sec,
            sources,
        } = v;
        Self {
            runtime_directory,
            global: GlobalConfig {
                min_keepalive,
                max_keepalive,
                peers,
            },
            updater: UpdaterConfig {
                cache_directory,
                refresh_sec,
            },
            sources,
        }
    }
}

#[inline]
const fn default_min_keepalive() -> u32 {
    10
}

#[inline]
const fn default_max_keepalive() -> u32 {
    0
}

#[inline]
const fn default_refresh_sec() -> u32 {
    1200
}
