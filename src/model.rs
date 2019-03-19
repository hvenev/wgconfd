// Copyright 2019 Hristo Venev
//
// See COPYING.

use base64;
use crate::bin;
use crate::ip::{Ipv4Addr, Ipv6Addr, Ipv4Net, Ipv6Net, NetParseError};
use std::{fmt};
use std::collections::{HashMap};
use std::str::FromStr;

pub type KeyParseError = base64::DecodeError;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Key([u8; 32]);

impl Key {
    pub fn from_bytes(s: &[u8]) -> Result<Key, KeyParseError> {
        let mut v = Key([0u8; 32]);
        let l = base64::decode_config_slice(s, base64::STANDARD, &mut v.0)?;
        if l != v.0.len() {
            return Err(base64::DecodeError::InvalidLength);
        }
        Ok(v)
    }
}

impl fmt::Display for Key {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", base64::display::Base64Display::with_config(&self.0, base64::STANDARD))
    }
}

impl FromStr for Key {
    type Err = KeyParseError;
    #[inline]
    fn from_str(s: &str) -> Result<Key, base64::DecodeError> {
        Key::from_bytes(s.as_bytes())
    }
}

impl serde::Serialize for Key {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        if ser.is_human_readable() {
            ser.collect_str(self)
        } else {
            ser.serialize_bytes(&self.0)
        }
    }
}

impl<'de> serde::Deserialize<'de> for Key {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        if de.is_human_readable() {
            struct KeyVisitor;
            impl<'de> serde::de::Visitor<'de> for KeyVisitor {
                type Value = Key;

                #[inline]
                fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                    f.write_str("WireGuard key")
                }

                #[inline]
                fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Self::Value, E> {
                    s.parse().map_err(E::custom)
                }
            }
            de.deserialize_str(KeyVisitor)
        } else {
            serde::Deserialize::deserialize(de).map(Key)
        }
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Endpoint {
    address: Ipv6Addr,
    port: u16,
}

impl Endpoint {
    #[inline]
    pub fn ipv6_address(&self) -> Ipv6Addr {
        self.address
    }

    #[inline]
    pub fn ipv4_address(&self) -> Option<Ipv4Addr> {
        let seg = self.address.octets();
        let (first, second) = array_refs![&seg, 12, 4];
        if *first == [0,0,0,0,0,0,0,0,0,0,0xff,0xff] {
            Some(Ipv4Addr::from(*second))
        } else {
            None
        }
    }

    #[inline]
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(ipv4) = self.ipv4_address() {
            write!(f, "{}:", ipv4)?;
        } else {
            write!(f, "[{}]:", self.ipv6_address())?;
        }
        write!(f, "{}", self.port())
    }
}

impl FromStr for Endpoint {
    type Err = NetParseError;
    fn from_str(s: &str) -> Result<Endpoint, NetParseError> {
        use std::net;
        net::SocketAddr::from_str(s)
            .map_err(|_| NetParseError)
            .map(|v| Endpoint {
                address: match v.ip() {
                    net::IpAddr::V4(a) => a.to_ipv6_mapped(),
                    net::IpAddr::V6(a) => a,
                },
                port: v.port(),
            })
    }
}

impl serde::Serialize for Endpoint {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        if ser.is_human_readable() {
            ser.collect_str(self)
        } else {
            let mut buf = [0u8; 16 + 2];
            let (buf_addr, buf_port) = mut_array_refs![&mut buf, 16, 2];
            *buf_addr = self.address.octets();
            *buf_port = crate::bin::u16_to_be(self.port);
            ser.serialize_bytes(&buf)
        }
    }
}

impl<'de> serde::Deserialize<'de> for Endpoint {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        if de.is_human_readable() {
            struct EndpointVisitor;
            impl<'de> serde::de::Visitor<'de> for EndpointVisitor {
                type Value = Endpoint;

                #[inline]
                fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                    f.write_str("IP:port")
                }

                #[inline]
                fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Self::Value, E> {
                    s.parse().map_err(E::custom)
                }
            }
            de.deserialize_str(EndpointVisitor)
        } else {
            let buf = <[u8; 16 + 2] as serde::Deserialize>::deserialize(de)?;
            let (buf_addr, buf_port) = array_refs![&buf, 16, 2];
            Ok(Endpoint {
                address: (*buf_addr).into(),
                port: bin::u16_from_be(*buf_port),
            })
        }
    }
}

#[derive(serde_derive::Serialize, serde_derive::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Peer {
    pub endpoint: Option<Endpoint>,
    pub psk: Option<Key>,
    pub keepalive: u32,
    pub ipv4: Vec<Ipv4Net>,
    pub ipv6: Vec<Ipv6Net>,
}

#[derive(serde_derive::Serialize, serde_derive::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Config {
    pub peers: HashMap<Key, Peer>,
}

impl Default for Config {
    #[inline]
    fn default() -> Config {
        Config {
            peers: HashMap::new(),
        }
    }
}
