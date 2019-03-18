use crate::ip::{Endpoint, Ipv4Net, Ipv6Net};
use crate::{config, proto};
use hash_map::HashMap;
use std::collections::hash_map;
use std::{error, fmt, io};

#[derive(Clone, PartialEq, Eq, Debug)]
struct Peer {
    endpoint: Endpoint,
    psk: Option<String>,
    keepalive: u32,
    ipv4: Vec<Ipv4Net>,
    ipv6: Vec<Ipv6Net>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Config {
    peers: HashMap<String, Peer>,
}

#[derive(Debug)]
pub struct ConfigError {
    pub url: String,
    pub peer: String,
    pub important: bool,
    err: &'static str,
}

impl error::Error for ConfigError {}
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} [{}] from [{}]: {}",
            if self.important {
                "Invalid peer"
            } else {
                "Misconfigured peer"
            },
            self.peer, self.url, self.err
        )
    }
}

impl Config {
    pub fn new() -> Config {
        Config {
            peers: HashMap::new(),
        }
    }

    pub fn add_peer(
        &mut self,
        errors: &mut Vec<ConfigError>,
        c: &config::PeerConfig,
        s: &config::Source,
        p: &proto::Peer,
    ) {
        if !valid_key(&p.public_key) {
            errors.push(ConfigError {
                url: s.url.clone(),
                peer: p.public_key.clone(),
                important: true,
                err: "Invalid public key",
            });
            return;
        }

        if let Some(ref psk) = s.psk {
            if !valid_key(psk) {
                errors.push(ConfigError {
                    url: s.url.clone(),
                    peer: p.public_key.clone(),
                    important: true,
                    err: "Invalid preshared key",
                });
                return;
            }
        }

        if c.omit_peers.contains(&p.public_key) {
            return;
        }

        let ent = match self.peers.entry(p.public_key.clone()) {
            hash_map::Entry::Occupied(ent) => {
                errors.push(ConfigError {
                    url: s.url.clone(),
                    peer: p.public_key.clone(),
                    important: true,
                    err: "Duplicate public key",
                });
                ent.into_mut()
            }
            hash_map::Entry::Vacant(ent) => {
                let mut keepalive = p.keepalive;
                if c.max_keepalive != 0 && (keepalive == 0 || keepalive > c.max_keepalive) {
                    keepalive = c.max_keepalive;
                }
                if keepalive != 0 && keepalive < c.min_keepalive {
                    keepalive = c.min_keepalive;
                }

                ent.insert(Peer {
                    endpoint: p.endpoint.clone(),
                    psk: s.psk.clone(),
                    keepalive,
                    ipv4: vec![],
                    ipv6: vec![],
                })
            },
        };

        let mut added = false;
        let mut removed = false;

        for i in p.ipv4.iter() {
            if s.ipv4.contains(i) {
                ent.ipv4.push(*i);
                added = true;
            } else {
                removed = true;
            }
        }
        for i in p.ipv6.iter() {
            if s.ipv6.contains(i) {
                ent.ipv6.push(*i);
                added = true;
            } else {
                removed = true;
            }
        }

        if removed {
            errors.push(ConfigError {
                url: s.url.clone(),
                peer: p.public_key.clone(),
                important: !added,
                err: if added {
                    "Some IPs removed"
                } else {
                    "All IPs removed"
                },
            });
        }
    }
}

impl Default for Config {
    #[inline]
    fn default() -> Self {
        Config::new()
    }
}

pub struct Device {
    ifname: String,
    wg_command: String,
}

impl Device {
    pub fn new(ifname: String, wg_command: String) -> Self {
        Device { ifname, wg_command }
    }

    pub fn apply_diff(&mut self, old: &Config, new: &Config) -> io::Result<()> {
        use std::process::{Command, Stdio};

        let mut proc = Command::new(&self.wg_command);
        proc.stdin(Stdio::piped());
        proc.arg("set");
        proc.arg(&self.ifname);

        let mut psks = Vec::<&str>::new();

        for (pubkey, conf) in new.peers.iter() {
            if let Some(old_peer) = old.peers.get(pubkey) {
                if *old_peer == *conf {
                    continue;
                }
            }
            proc.arg("peer");
            proc.arg(pubkey);

            // TODO: maybe skip endpoint?
            proc.arg("endpoint");
            proc.arg(format!("{}", conf.endpoint));

            if let Some(psk) = &conf.psk {
                proc.arg("preshared-key");
                proc.arg("/dev/stdin");
                psks.push(psk);
            }

            let mut ips = String::new();
            {
                use std::fmt::Write;
                for ip in conf.ipv4.iter() {
                    if !ips.is_empty() {
                        ips.push(',');
                    }
                    write!(ips, "{}", ip).unwrap();
                }
                for ip in conf.ipv6.iter() {
                    if !ips.is_empty() {
                        ips.push(',');
                    }
                    write!(ips, "{}", ip).unwrap();
                }
            }

            proc.arg("allowed-ips");
            proc.arg(ips);
        }

        for pubkey in old.peers.keys() {
            if new.peers.contains_key(pubkey) {
                continue;
            }
            proc.arg("peer");
            proc.arg(pubkey);
            proc.arg("remove");
        }

        let mut proc = proc.spawn()?;
        {
            use std::io::Write;
            let stdin = proc.stdin.as_mut().unwrap();
            for psk in psks {
                writeln!(stdin, "{}", psk)?;
            }
        }

        let r = proc.wait()?;
        if !r.success() {
            return Err(io::Error::new(io::ErrorKind::Other, "Child process failed"));
        }
        Ok(())
    }
}

fn valid_key(s: &str) -> bool {
    let s = s.as_bytes();
    if s.len() != 44 {
        return false;
    }
    if s[43] != b'=' {
        return false;
    }
    for c in s[0..42].iter().cloned() {
        if c >= b'0' && c <= b'9' {
            continue;
        }
        if c >= b'A' && c <= b'Z' {
            continue;
        }
        if c >= b'a' && c <= b'z' {
            continue;
        }
        if c == b'+' || c <= b'/' {
            continue;
        }
        return false;
    }
    b"048AEIMQUYcgkosw".contains(&s[42])
}
