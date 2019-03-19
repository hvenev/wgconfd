// Copyright 2019 Hristo Venev
//
// See COPYING.

use crate::bin;
use serde;
use std::iter::{FromIterator, IntoIterator};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::str::FromStr;
use std::{error, fmt, iter, net};

#[derive(Debug)]
pub struct NetParseError {}

impl error::Error for NetParseError {}
impl fmt::Display for NetParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Invalid IP network")
    }
}

macro_rules! per_proto {
    ($nett:ident ($addrt:ident; $expecting:expr); $intt:ident($bytes:expr); $sett:ident) => {
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        pub struct $nett {
            pub address: $addrt,
            pub prefix_len: u8,
        }

        impl $nett {
            const BITS: u8 = $bytes * 8;

            pub fn contains(&self, other: &$nett) -> bool {
                if self.prefix_len > other.prefix_len {
                    return false;
                }
                if self.prefix_len == other.prefix_len {
                    return self.address == other.address;
                }
                if self.prefix_len == 0 {
                    return true;
                }
                // self.prefix_len < other.prefix_len = BITS
                let shift = Self::BITS - self.prefix_len;
                let v1: $intt = self.address.into();
                let v2: $intt = other.address.into();
                v1 >> shift == v2 >> shift
            }
        }

        impl fmt::Display for $nett {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "{}/{}", self.address, self.prefix_len)
            }
        }

        impl FromStr for $nett {
            type Err = NetParseError;
            fn from_str(s: &str) -> Result<$nett, NetParseError> {
                let (addr, pfx) = pfx_split(s)?;
                let addr = $addrt::from_str(addr).map_err(|_| NetParseError {})?;

                let r = $nett {
                    address: addr,
                    prefix_len: pfx,
                };
                if !r.is_valid() {
                    return Err(NetParseError {});
                }
                Ok(r)
            }
        }

        impl serde::Serialize for $nett {
            fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
                if ser.is_human_readable() {
                    ser.serialize_str(&format!("{}", self))
                } else {
                    let mut buf = [0u8; $bytes + 1];
                    *array_mut_ref![&mut buf, 0, $bytes] = self.address.octets();
                    buf[$bytes] = self.prefix_len;
                    ser.serialize_bytes(&buf)
                }
            }
        }

        impl<'de> serde::Deserialize<'de> for $nett {
            fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
                if de.is_human_readable() {
                    struct NetVisitor;
                    impl<'de> serde::de::Visitor<'de> for NetVisitor {
                        type Value = $nett;

                        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                            f.write_str($expecting)
                        }

                        fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Self::Value, E> {
                            s.parse().map_err(E::custom)
                        }
                    }
                    de.deserialize_str(NetVisitor)
                } else {
                    let buf = <[u8; $bytes + 1] as serde::Deserialize>::deserialize(de)?;
                    let r = $nett {
                        address: (*array_ref![&buf, 0, $bytes]).into(),
                        prefix_len: buf[$bytes],
                    };
                    if r.is_valid() {
                        return Err(serde::de::Error::custom(NetParseError {}));
                    }
                    Ok(r)
                }
            }
        }

        #[derive(Clone, PartialEq, Eq, PartialOrd, Hash, Debug)]
        pub struct $sett {
            nets: Vec<$nett>,
        }

        impl Default for $sett {
            #[inline]
            fn default() -> Self {
                $sett::new()
            }
        }

        impl $sett {
            #[inline]
            pub fn new() -> Self {
                $sett { nets: vec![] }
            }

            #[inline]
            fn siblings(a: &$nett, b: &$nett) -> bool {
                let pfx = a.prefix_len;
                if b.prefix_len != pfx || pfx == 0 {
                    return false;
                }
                let a: $intt = a.address.into();
                let b: $intt = b.address.into();
                a ^ b == 1 << ($nett::BITS - pfx)
            }

            pub fn insert(&mut self, mut net: $nett) {
                let mut i = match self.nets.binary_search(&net) {
                    Err(i) => i,
                    Ok(_) => {
                        return;
                    }
                };
                let mut j = i;
                if i != 0 && self.nets[i - 1].contains(&net) {
                    net = self.nets[i - 1];
                    i -= 1;
                }
                while j < self.nets.len() && net.contains(&self.nets[j]) {
                    j += 1;
                }
                loop {
                    if j < self.nets.len() && Self::siblings(&net, &self.nets[j]) {
                        j += 1;
                    } else if i != 0 && Self::siblings(&self.nets[i - 1], &net) {
                        net = self.nets[i - 1];
                        i -= 1;
                    } else {
                        break;
                    }
                    net.prefix_len -= 1;
                }
                self.nets.splice(i..j, iter::once(net));
            }

            pub fn contains(&self, net: &$nett) -> bool {
                match self.nets.binary_search(&net) {
                    Err(i) => {
                        if i == 0 {
                            return false;
                        }
                        self.nets[i - 1].contains(&net)
                    }
                    Ok(_) => true,
                }
            }

            #[inline]
            pub fn iter(&self) -> std::slice::Iter<$nett> {
                self.nets.iter()
            }
        }

        impl IntoIterator for $sett {
            type Item = $nett;
            type IntoIter = std::vec::IntoIter<$nett>;

            #[inline]
            fn into_iter(self) -> Self::IntoIter {
                self.nets.into_iter()
            }
        }

        impl FromIterator<$nett> for $sett {
            fn from_iter<I: IntoIterator<Item = $nett>>(it: I) -> $sett {
                let mut r = $sett::new();
                for net in it {
                    r.insert(net);
                }
                r
            }
        }

        impl<'a> From<$nett> for $sett {
            #[inline]
            fn from(v: $nett) -> $sett {
                $sett { nets: vec![v] }
            }
        }

        impl<'a> From<[$nett; 1]> for $sett {
            #[inline]
            fn from(v: [$nett; 1]) -> $sett {
                $sett { nets: vec![v[0]] }
            }
        }

        impl From<$sett> for Vec<$nett> {
            fn from(v: $sett) -> Vec<$nett> {
                v.nets
            }
        }

        impl From<Vec<$nett>> for $sett {
            fn from(nets: Vec<$nett>) -> $sett {
                let mut s = $sett { nets };
                let len = s.nets.len();
                if len == 0 {
                    return s;
                }
                s.nets.sort();
                let mut i = 1;
                for j in 1..len {
                    let mut net = s.nets[j];
                    if s.nets[i - 1].contains(&net) {
                        net = s.nets[i - 1];
                        i -= 1;
                    }
                    while i != 0 && Self::siblings(&s.nets[i - 1], &net) {
                        net = s.nets[i - 1];
                        net.prefix_len -= 1;
                        i -= 1;
                    }
                    s.nets[i] = net;
                    i += 1;
                }
                s.nets.splice(i.., iter::empty());
                s
            }
        }

        impl<'a> From<&'a [$nett]> for $sett {
            #[inline]
            fn from(nets: &'a [$nett]) -> $sett {
                Vec::from(nets).into()
            }
        }

        impl<'a> From<&'a mut [$nett]> for $sett {
            #[inline]
            fn from(nets: &'a mut [$nett]) -> $sett {
                Vec::from(nets).into()
            }
        }

        impl serde::Serialize for $sett {
            fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
                <Vec<$nett> as serde::Serialize>::serialize(&self.nets, ser)
            }
        }

        impl<'de> serde::Deserialize<'de> for $sett {
            fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
                <Vec<$nett> as serde::Deserialize>::deserialize(de).map($sett::from)
            }
        }
    };
}

per_proto!(Ipv4Net(Ipv4Addr; "IPv4 network"); u32(4); Ipv4Set);
per_proto!(Ipv6Net(Ipv6Addr; "IPv6 network"); u128(16); Ipv6Set);

impl Ipv4Net {
    pub fn is_valid(&self) -> bool {
        let pfx = self.prefix_len;
        if pfx > 32 {
            return false;
        }
        if pfx == 32 {
            return true;
        }
        let val: u32 = self.address.into();
        val & (u32::max_value() >> pfx) == 0
    }
}

impl Ipv6Net {
    pub fn is_valid(&self) -> bool {
        let pfx = self.prefix_len;
        if pfx > 128 {
            return false;
        }
        if pfx == 128 {
            return true;
        }

        let val: u128 = self.address.into();
        let val: [u64; 2] = [(val >> 64) as u64, val as u64];
        if pfx >= 64 {
            return val[1] & (u64::max_value() >> (pfx - 64)) == 0;
        }
        if val[1] != 0 {
            return false;
        }
        val[0] & (u64::max_value() >> pfx) == 0
    }
}

fn pfx_split(s: &str) -> Result<(&str, u8), NetParseError> {
    let i = match s.find('/') {
        Some(i) => i,
        None => {
            return Err(NetParseError {});
        }
    };
    let (addr, pfx) = s.split_at(i);
    let pfx = u8::from_str(&pfx[1..]).map_err(|_| NetParseError {})?;
    Ok((addr, pfx))
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Endpoint {
    pub address: Ipv6Addr,
    pub port: u16,
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.address.segments()[5] == 0xffff {
            write!(f, "{}:", self.address.to_ipv4().unwrap())?;
        } else {
            write!(f, "[{}]:", self.address)?;
        }
        write!(f, "{}", self.port)
    }
}

impl FromStr for Endpoint {
    type Err = net::AddrParseError;
    fn from_str(s: &str) -> Result<Endpoint, net::AddrParseError> {
        net::SocketAddr::from_str(s).map(|v| Endpoint {
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
            ser.serialize_str(&format!("{}", self))
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

                fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                    f.write_str("ip:port")
                }

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
