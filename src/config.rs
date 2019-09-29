// Copyright 2019 Hristo Venev
//
// See COPYING.

use crate::model::{Ipv4Set, Ipv6Set, Key};
use serde_derive;
use std::collections::HashMap;
use std::path::PathBuf;

#[serde(deny_unknown_fields)]
#[derive(serde_derive::Serialize, serde_derive::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Source {
    pub url: String,
    pub psk: Option<Key>,
    pub ipv4: Ipv4Set,
    pub ipv6: Ipv6Set,
    #[serde(default)]
    pub required: bool,
}

#[serde(deny_unknown_fields)]
#[derive(serde_derive::Serialize, serde_derive::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Peer {
    pub source: Option<String>,
    pub psk: Option<Key>,
}

#[serde(deny_unknown_fields)]
#[derive(serde_derive::Serialize, serde_derive::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct GlobalConfig {
    #[serde(default = "default_min_keepalive")]
    pub min_keepalive: u32,
    #[serde(default = "default_max_keepalive")]
    pub max_keepalive: u32,
    #[serde(default, rename = "peer")]
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

#[serde(deny_unknown_fields)]
#[derive(serde_derive::Serialize, serde_derive::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct UpdaterConfig {
    pub cache_directory: Option<PathBuf>,

    // Number of seconds between regular updates.
    #[serde(default = "default_refresh_sec")]
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

#[serde(deny_unknown_fields)]
#[derive(serde_derive::Serialize, serde_derive::Deserialize, Default, Clone, Debug)]
pub struct Config {
    pub runtime_directory: Option<PathBuf>,

    #[serde(flatten)]
    pub global: GlobalConfig,

    #[serde(flatten)]
    pub updater: UpdaterConfig,

    #[serde(rename = "source")]
    pub sources: HashMap<String, Source>,
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
