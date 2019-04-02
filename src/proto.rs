// Copyright 2019 Hristo Venev
//
// See COPYING.

use crate::model::{Endpoint, Ipv4Net, Ipv6Net, Key};
use serde_derive;
use std::time::SystemTime;

#[serde(deny_unknown_fields)]
#[derive(serde_derive::Serialize, serde_derive::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Peer {
    pub public_key: Key,
    #[serde(default)]
    pub ipv4: Vec<Ipv4Net>,
    #[serde(default)]
    pub ipv6: Vec<Ipv6Net>,
}

#[serde(deny_unknown_fields)]
#[derive(serde_derive::Serialize, serde_derive::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Server {
    #[serde(flatten)]
    pub peer: Peer,
    pub endpoint: Endpoint,
    #[serde(default)]
    pub keepalive: u32,
}

#[serde(deny_unknown_fields)]
#[derive(serde_derive::Serialize, serde_derive::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct RoadWarrior {
    #[serde(flatten)]
    pub peer: Peer,
    pub base: Key,
}

#[derive(serde_derive::Serialize, serde_derive::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct SourceConfig {
    #[serde(default)]
    pub servers: Vec<Server>,
    #[serde(default)]
    pub road_warriors: Vec<RoadWarrior>,
}

#[derive(serde_derive::Serialize, serde_derive::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct SourceNextConfig {
    #[serde(with = "serde_utc")]
    pub update_at: SystemTime,
    #[serde(flatten)]
    pub config: SourceConfig,
}

#[derive(serde_derive::Serialize, serde_derive::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Source {
    #[serde(flatten)]
    pub config: SourceConfig,
    pub next: Option<SourceNextConfig>,
}

impl Source {
    pub fn empty() -> Source {
        Source {
            config: SourceConfig {
                servers: vec![],
                road_warriors: vec![],
            },
            next: None,
        }
    }
}

mod serde_utc {
    use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
    use serde::*;
    use std::fmt;
    use std::time::SystemTime;

    pub fn serialize<S: Serializer>(t: &SystemTime, ser: S) -> Result<S::Ok, S::Error> {
        let t = DateTime::<Utc>::from(*t);
        if ser.is_human_readable() {
            ser.serialize_str(&t.to_rfc3339_opts(SecondsFormat::Nanos, true))
        } else {
            let mut buf = [0u8; 12];
            let (buf_secs, buf_nanos) = mut_array_refs![&mut buf, 8, 4];
            *buf_secs = t.timestamp().to_be_bytes();
            *buf_nanos = t.timestamp_subsec_nanos().to_be_bytes();
            ser.serialize_bytes(&buf)
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<SystemTime, D::Error> {
        if de.is_human_readable() {
            struct RFC3339Visitor;
            impl<'de> serde::de::Visitor<'de> for RFC3339Visitor {
                type Value = SystemTime;

                fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                    f.write_str("RFC3339 time")
                }

                fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Self::Value, E> {
                    DateTime::parse_from_rfc3339(s)
                        .map_err(de::Error::custom)
                        .map(SystemTime::from)
                }
            }
            de.deserialize_str(RFC3339Visitor)
        } else {
            let mut buf = <[u8; 12]>::deserialize(de)?;
            let (buf_secs, buf_nanos) = array_refs![&mut buf, 8, 4];
            let secs = i64::from_be_bytes(*buf_secs);
            let nanos = u32::from_be_bytes(*buf_nanos);
            Ok(Utc.timestamp(secs, nanos).into())
        }
    }
}
