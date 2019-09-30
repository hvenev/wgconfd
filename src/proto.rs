// SPDX-License-Identifier: LGPL-3.0-or-later
//
// Copyright 2019 Hristo Venev

use crate::model::{Endpoint, Ipv4Net, Ipv6Net, Key};
use serde_derive;
use std::time::SystemTime;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Peer {
    pub public_key: Key,
    pub ipv4: Vec<Ipv4Net>,
    pub ipv6: Vec<Ipv6Net>,
}

#[serde(from = "ServerRepr", into = "ServerRepr")]
#[derive(serde_derive::Serialize, serde_derive::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Server {
    pub peer: Peer,
    pub endpoint: Endpoint,
    pub keepalive: u32,
}

#[derive(serde_derive::Serialize, serde_derive::Deserialize)]
#[serde(deny_unknown_fields)]
struct ServerRepr {
    public_key: Key,
    #[serde(default)]
    ipv4: Vec<Ipv4Net>,
    #[serde(default)]
    ipv6: Vec<Ipv6Net>,
    endpoint: Endpoint,
    #[serde(default)]
    keepalive: u32,
}

impl From<Server> for ServerRepr {
    #[inline]
    fn from(v: Server) -> Self {
        let Server {
            peer,
            endpoint,
            keepalive,
        } = v;
        let Peer {
            public_key,
            ipv4,
            ipv6,
        } = peer;
        Self {
            public_key,
            ipv4,
            ipv6,
            endpoint,
            keepalive,
        }
    }
}

impl From<ServerRepr> for Server {
    #[inline]
    fn from(v: ServerRepr) -> Self {
        let ServerRepr {
            public_key,
            ipv4,
            ipv6,
            endpoint,
            keepalive,
        } = v;
        Self {
            peer: Peer {
                public_key,
                ipv4,
                ipv6,
            },
            endpoint,
            keepalive,
        }
    }
}

#[derive(serde_derive::Serialize, serde_derive::Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(from = "RoadWarriorRepr", into = "RoadWarriorRepr")]
pub struct RoadWarrior {
    pub peer: Peer,
    pub base: Key,
}

#[derive(serde_derive::Serialize, serde_derive::Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(deny_unknown_fields)]
pub struct RoadWarriorRepr {
    public_key: Key,
    #[serde(default)]
    ipv4: Vec<Ipv4Net>,
    #[serde(default)]
    ipv6: Vec<Ipv6Net>,
    pub base: Key,
}

impl From<RoadWarrior> for RoadWarriorRepr {
    #[inline]
    fn from(v: RoadWarrior) -> Self {
        let RoadWarrior { peer, base } = v;
        let Peer {
            public_key,
            ipv4,
            ipv6,
        } = peer;
        Self {
            public_key,
            ipv4,
            ipv6,
            base,
        }
    }
}

impl From<RoadWarriorRepr> for RoadWarrior {
    #[inline]
    fn from(v: RoadWarriorRepr) -> Self {
        let RoadWarriorRepr {
            public_key,
            ipv4,
            ipv6,
            base,
        } = v;
        Self {
            peer: Peer {
                public_key,
                ipv4,
                ipv6,
            },
            base,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SourceConfig {
    pub servers: Vec<Server>,
    pub road_warriors: Vec<RoadWarrior>,
}

#[derive(serde_derive::Serialize, serde_derive::Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(from = "SourceRepr", into = "SourceRepr")]
pub struct Source {
    pub config: SourceConfig,
    pub next: Option<(SystemTime, SourceConfig)>,
}

impl Source {
    pub fn empty() -> Self {
        Self {
            config: SourceConfig {
                servers: vec![],
                road_warriors: vec![],
            },
            next: None,
        }
    }
}

#[derive(serde_derive::Serialize, serde_derive::Deserialize, Clone, PartialEq, Eq, Debug)]
struct SourceNextRepr {
    #[serde(default)]
    servers: Vec<Server>,
    #[serde(default)]
    road_warriors: Vec<RoadWarrior>,
    #[serde(with = "serde_utc")]
    update_at: SystemTime,
}

#[derive(serde_derive::Serialize, serde_derive::Deserialize, Clone, PartialEq, Eq, Debug)]
struct SourceRepr {
    #[serde(default)]
    servers: Vec<Server>,
    #[serde(default)]
    road_warriors: Vec<RoadWarrior>,
    next: Option<SourceNextRepr>,
}

impl From<Source> for SourceRepr {
    #[inline]
    fn from(v: Source) -> Self {
        let Source { config, next } = v;
        let SourceConfig {
            servers,
            road_warriors,
        } = config;
        Self {
            servers,
            road_warriors,
            next: next.map(
                #[inline]
                |next| {
                    let (update_at, next) = next;
                    SourceNextRepr {
                        servers: next.servers,
                        road_warriors: next.road_warriors,
                        update_at,
                    }
                },
            ),
        }
    }
}

impl From<SourceRepr> for Source {
    #[inline]
    fn from(v: SourceRepr) -> Self {
        let SourceRepr {
            servers,
            road_warriors,
            next,
        } = v;
        Self {
            config: SourceConfig {
                servers,
                road_warriors,
            },
            next: next.map(
                #[inline]
                |next| {
                    let SourceNextRepr {
                        servers,
                        road_warriors,
                        update_at,
                    } = next;
                    (
                        update_at,
                        SourceConfig {
                            servers,
                            road_warriors,
                        },
                    )
                },
            ),
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
            let mut buf = [0_u8; 12];
            // FIXME: arrayref needs to silence this per-expression
            #[allow(clippy::eval_order_dependence)]
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

                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
            // FIXME: arrayref needs to silence this per-expression
            #[allow(clippy::eval_order_dependence)]
            let (buf_secs, buf_nanos) = array_refs![&mut buf, 8, 4];
            let secs = i64::from_be_bytes(*buf_secs);
            let nanos = u32::from_be_bytes(*buf_nanos);
            Ok(Utc.timestamp(secs, nanos).into())
        }
    }
}
