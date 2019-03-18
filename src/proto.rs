use ::std::time::SystemTime;
use ::serde_derive;

use crate::ip::{Ipv4Net, Ipv6Net, Endpoint};

#[serde(deny_unknown_fields)]
#[derive(serde_derive::Serialize, serde_derive::Deserialize)]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Peer {
    pub public_key: String,
    pub endpoint: Endpoint,
    #[serde(default = "default_peer_keepalive")]
    pub keepalive: u32,
    pub ipv4: Vec<Ipv4Net>,
    pub ipv6: Vec<Ipv6Net>,
}

fn default_peer_keepalive() -> u32 {
    0
}

#[derive(serde_derive::Serialize, serde_derive::Deserialize)]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SourceConfig {
    pub peers: Vec<Peer>,
}

#[derive(serde_derive::Serialize, serde_derive::Deserialize)]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SourceNextConfig {
    #[serde(with = "serde_utc")]
    pub update_at: SystemTime,
    #[serde(flatten)]
    pub config: SourceConfig,
}

#[derive(serde_derive::Serialize, serde_derive::Deserialize)]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Source {
    #[serde(flatten)]
    pub config: SourceConfig,
    pub next: Option<SourceNextConfig>,
}

mod serde_utc {
    use ::std::time::SystemTime;
    use ::chrono::{DateTime, TimeZone, Utc, SecondsFormat};
    use ::serde::*;
    use crate::bin;

    pub fn serialize<S: Serializer>(t: &SystemTime, ser: S) -> Result<S::Ok, S::Error> {
        let t = DateTime::<Utc>::from(*t);
        if ser.is_human_readable() {
            ser.serialize_str(&t.to_rfc3339_opts(SecondsFormat::Nanos, true))
        } else {
            let mut buf = [0u8; 12];
            let (buf_secs, buf_nanos) = mut_array_refs![&mut buf, 8, 4];
            *buf_secs = bin::i64_to_be(t.timestamp());
            *buf_nanos = bin::u32_to_be(t.timestamp_subsec_nanos());
            ser.serialize_bytes(&buf)
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<SystemTime, D::Error> {
        if de.is_human_readable() {
            let s: String = String::deserialize(de)?;
            let t = DateTime::parse_from_rfc3339(&s).map_err(de::Error::custom)?;
            Ok(t.into())
        } else {
            let mut buf = <[u8; 12]>::deserialize(de)?;
            let (buf_secs, buf_nanos) = array_refs![&mut buf, 8, 4];
            let secs = bin::i64_from_be(*buf_secs);
            let nanos = bin::u32_from_be(*buf_nanos);
            Ok(Utc.timestamp(secs, nanos).into())
        }
    }
}
