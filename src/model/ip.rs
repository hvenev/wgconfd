// SPDX-License-Identifier: LGPL-3.0-or-later
//
// Copyright 2019 Hristo Venev

use serde;
use std::iter::{FromIterator, IntoIterator};
pub use std::net::{Ipv4Addr, Ipv6Addr};
use std::str::FromStr;
use std::{error, fmt, iter};

#[derive(Debug)]
pub struct NetParseError;

impl error::Error for NetParseError {}
impl fmt::Display for NetParseError {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Invalid address")
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

            pub fn is_valid(&self) -> bool {
                let pfx = self.prefix_len;
                if pfx > Self::BITS {
                    return false;
                }
                if pfx == Self::BITS {
                    return true;
                }
                let val: $intt = self.address.into();
                val & ($intt::max_value() >> pfx) == 0
            }

            pub fn contains(&self, other: &Self) -> bool {
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
            #[inline]
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}/{}", self.address, self.prefix_len)
            }
        }

        impl FromStr for $nett {
            type Err = NetParseError;
            fn from_str(s: &str) -> Result<Self, NetParseError> {
                let (addr, pfx) = pfx_split(s)?;
                let addr = $addrt::from_str(addr).map_err(|_| NetParseError)?;

                let r = Self {
                    address: addr,
                    prefix_len: pfx,
                };
                if !r.is_valid() {
                    return Err(NetParseError);
                }
                Ok(r)
            }
        }

        impl serde::Serialize for $nett {
            fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
                if ser.is_human_readable() {
                    ser.collect_str(self)
                } else {
                    let mut buf = [0_u8; $bytes + 1];
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

                        #[inline]
                        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                            f.write_str($expecting)
                        }

                        #[inline]
                        fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Self::Value, E> {
                            s.parse().map_err(E::custom)
                        }
                    }
                    de.deserialize_str(NetVisitor)
                } else {
                    let buf = <[u8; $bytes + 1] as serde::Deserialize>::deserialize(de)?;
                    let r = Self {
                        address: (*array_ref![&buf, 0, $bytes]).into(),
                        prefix_len: buf[$bytes],
                    };
                    if r.is_valid() {
                        return Err(serde::de::Error::custom(NetParseError));
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
                Self::new()
            }
        }

        impl $sett {
            #[inline]
            pub fn new() -> Self {
                Self { nets: vec![] }
            }

            #[inline]
            fn siblings(a: $nett, b: $nett) -> bool {
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
                    Err(v) => v,
                    Ok(_) => return,
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
                    if j < self.nets.len() && Self::siblings(net, self.nets[j]) {
                        j += 1;
                    } else if i != 0 && Self::siblings(self.nets[i - 1], net) {
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
            pub fn iter(&self) -> std::slice::Iter<'_, $nett> {
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

        impl<'a> IntoIterator for &'a $sett {
            type Item = &'a $nett;
            type IntoIter = std::slice::Iter<'a, $nett>;

            #[inline]
            fn into_iter(self) -> Self::IntoIter {
                self.nets.iter()
            }
        }

        impl FromIterator<$nett> for $sett {
            #[inline]
            fn from_iter<I: IntoIterator<Item = $nett>>(it: I) -> Self {
                let mut r = Self::new();
                for net in it {
                    r.insert(net);
                }
                r
            }
        }

        impl<'a> From<$nett> for $sett {
            #[inline]
            fn from(v: $nett) -> Self {
                Self { nets: vec![v] }
            }
        }

        impl<'a> From<[$nett; 1]> for $sett {
            #[inline]
            fn from(v: [$nett; 1]) -> Self {
                Self { nets: vec![v[0]] }
            }
        }

        impl From<$sett> for Vec<$nett> {
            fn from(v: $sett) -> Self {
                v.nets
            }
        }

        impl From<Vec<$nett>> for $sett {
            fn from(nets: Vec<$nett>) -> Self {
                let mut s = Self { nets };
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
                    while i != 0 && Self::siblings(s.nets[i - 1], net) {
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
            fn from(nets: &'a [$nett]) -> Self {
                Vec::from(nets).into()
            }
        }

        impl<'a> From<&'a mut [$nett]> for $sett {
            #[inline]
            fn from(nets: &'a mut [$nett]) -> Self {
                Vec::from(nets).into()
            }
        }

        impl serde::Serialize for $sett {
            #[inline]
            fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
                <Vec<$nett> as serde::Serialize>::serialize(&self.nets, ser)
            }
        }

        impl<'de> serde::Deserialize<'de> for $sett {
            #[inline]
            fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
                <Vec<$nett> as serde::Deserialize>::deserialize(de).map(Self::from)
            }
        }
    };
}

per_proto!(Ipv4Net(Ipv4Addr; "IPv4 network"); u32(4); Ipv4Set);
per_proto!(Ipv6Net(Ipv6Addr; "IPv6 network"); u128(16); Ipv6Set);

fn pfx_split(s: &str) -> Result<(&str, u8), NetParseError> {
    let i = match s.find('/') {
        Some(v) => v,
        None => return Err(NetParseError),
    };
    let (addr, pfx) = s.split_at(i);
    let pfx = u8::from_str(&pfx[1..]).map_err(|_| NetParseError)?;
    Ok((addr, pfx))
}

#[cfg(test)]
mod test {
    use super::{pfx_split, Ipv4Addr, Ipv4Net, Ipv4Set, Ipv6Addr, Ipv6Net};
    use std::str::FromStr;

    #[test]
    fn test_pfx_split() {
        assert_eq!(pfx_split("asdf/0").unwrap(), ("asdf", 0));
        assert_eq!(pfx_split("asdf/123").unwrap(), ("asdf", 123));
        assert_eq!(pfx_split("asdf/0123").unwrap(), ("asdf", 123));
        assert_eq!(pfx_split("/1").unwrap(), ("", 1));
        assert_eq!(pfx_split("abc/2").unwrap(), ("abc", 2));

        assert!(pfx_split("no_slash").is_err());
        assert!(pfx_split("asdf/abc").is_err());
        assert!(pfx_split("asdf/0abc").is_err());
        assert!(pfx_split("asdf/0x123").is_err());
        assert!(pfx_split("asdf/12345").is_err());
    }

    #[test]
    fn test_net_parse() {
        assert_eq!(
            Ipv4Net::from_str("192.0.2.5/32").unwrap(),
            Ipv4Net {
                address: Ipv4Addr::from_str("192.0.2.5").unwrap(),
                prefix_len: 32,
            }
        );

        assert!(Ipv4Net::from_str("error").is_err());

        assert!(Ipv4Net::from_str("192.0.2.128/32").is_ok());
        assert!(Ipv4Net::from_str("192.0.2.128/25").is_ok());
        assert!(Ipv4Net::from_str("192.0.2.128/24").is_err());
        assert!(Ipv4Net::from_str("192.0.2.128").is_err());
    }

    #[test]
    fn test_net_display() {
        assert_eq!(
            (Ipv4Net {
                address: Ipv4Addr::from_str("192.0.2.0").unwrap(),
                prefix_len: 28,
            })
            .to_string(),
            "192.0.2.0/28"
        );

        assert_eq!(
            (Ipv6Net {
                address: Ipv6Addr::from_str("::1").unwrap(),
                prefix_len: 128,
            })
            .to_string(),
            "::1/128"
        );
    }

    fn disp_set(s: &Ipv4Set) -> String {
        s.iter()
            .map(Ipv4Net::to_string)
            .collect::<Vec<_>>()
            .join(",")
    }

    #[test]
    fn test_set_insert() {
        let mut s = Ipv4Set::default();
        assert_eq!(disp_set(&s), "");

        s.insert(Ipv4Net::from_str("192.0.2.7/32").unwrap());
        assert_eq!(disp_set(&s), "192.0.2.7/32");

        s.insert(Ipv4Net::from_str("192.0.2.5/32").unwrap());
        assert_eq!(disp_set(&s), "192.0.2.5/32,192.0.2.7/32");

        s.insert(Ipv4Net::from_str("192.0.2.6/32").unwrap());
        assert_eq!(disp_set(&s), "192.0.2.5/32,192.0.2.6/31");

        let mut s1 = s.clone();
        s1.insert(Ipv4Net::from_str("192.0.2.0/30").unwrap());
        assert_eq!(disp_set(&s1), "192.0.2.0/30,192.0.2.5/32,192.0.2.6/31");

        s.insert(Ipv4Net::from_str("192.0.2.4/32").unwrap());
        assert_eq!(disp_set(&s), "192.0.2.4/30");

        s1.insert(Ipv4Net::from_str("192.0.2.4/32").unwrap());
        assert_eq!(disp_set(&s1), "192.0.2.0/29");

        s.insert(Ipv4Net::from_str("0.0.0.0/0").unwrap());
        assert_eq!(disp_set(&s), "0.0.0.0/0");
    }

    #[test]
    fn test_set_from_slice() {
        fn s(v: &[&str]) -> String {
            disp_set(&Ipv4Set::from(
                v.iter()
                    .cloned()
                    .map(Ipv4Net::from_str)
                    .map(Result::unwrap)
                    .collect::<Vec<_>>(),
            ))
        }

        assert_eq!(s(&[]), "");
        assert_eq!(s(&["192.0.2.7/32"]), "192.0.2.7/32");
        assert_eq!(s(&["192.0.2.7/32", "192.0.2.7/32"]), "192.0.2.7/32");
        assert_eq!(
            s(&["192.0.2.7/32", "192.0.2.5/32"]),
            "192.0.2.5/32,192.0.2.7/32"
        );
        assert_eq!(
            s(&["192.0.2.7/32", "192.0.2.5/32", "192.0.2.6/32"]),
            "192.0.2.5/32,192.0.2.6/31"
        );
        assert_eq!(
            s(&[
                "192.0.2.7/32",
                "192.0.2.5/32",
                "192.0.2.6/32",
                "192.0.2.4/32"
            ]),
            "192.0.2.4/30"
        );
        assert_eq!(
            s(&["192.0.2.7/32", "192.0.2.6/32", "192.0.2.5/32", "0.0.0.0/0"]),
            "0.0.0.0/0"
        );
    }
}
