use crate::ip::{Ipv4Set, Ipv6Set};
use serde_derive;

#[serde(deny_unknown_fields)]
#[derive(serde_derive::Serialize, serde_derive::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Source {
    pub url: String,
    pub psk: Option<String>,
    pub ipv4: Ipv4Set,
    pub ipv6: Ipv6Set,
}

#[serde(deny_unknown_fields)]
#[derive(serde_derive::Serialize, serde_derive::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct PeerConfig {
    pub own_public_key: String,
    #[serde(default = "default_min_keepalive")]
    pub min_keepalive: u32,
    #[serde(default = "default_max_keepalive")]
    pub max_keepalive: u32,
}

#[serde(deny_unknown_fields)]
#[derive(serde_derive::Serialize, serde_derive::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct UpdateConfig {
    // Number of seconds between regular updates.
    #[serde(default = "default_refresh")]
    pub refresh_period: u32,
}

#[serde(deny_unknown_fields)]
#[derive(serde_derive::Serialize, serde_derive::Deserialize, Clone, Debug)]
pub struct Config {
    pub ifname: String,
    #[serde(default = "default_wg_command")]
    pub wg_command: String,

    #[serde(flatten)]
    pub peers: PeerConfig,

    #[serde(flatten)]
    pub update: UpdateConfig,

    pub sources: Vec<Source>,
}

impl PeerConfig {
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

fn default_wg_command() -> String {
    "wg".to_owned()
}

fn default_min_keepalive() -> u32 {
    10
}

fn default_max_keepalive() -> u32 {
    0
}

fn default_refresh() -> u32 {
    1200
}
