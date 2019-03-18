use crate::ip::{Endpoint, Ipv4Net, Ipv6Net};
use crate::{config, proto};
use hash_map::HashMap;
use std::collections::hash_map;
use std::{error, fmt, io};

#[derive(Debug)]
pub struct ConfigError {
    pub url: String,
    pub peer: String,
    pub important: bool,
    err: &'static str,
}

impl ConfigError {
    fn new(err: &'static str, s: &config::Source, p: &proto::Peer, important: bool) -> Self {
        ConfigError {
            url: s.url.clone(),
            peer: p.public_key.clone(),
            important,
            err,
        }
    }
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
            self.peer,
            self.url,
            self.err
        )
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
struct Peer {
    endpoint: Option<Endpoint>,
    psk: Option<String>,
    keepalive: u32,
    ipv4: Vec<Ipv4Net>,
    ipv6: Vec<Ipv6Net>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Config {
    peers: HashMap<String, Peer>,
}

pub struct ConfigBuilder<'a> {
    peers: HashMap<String, Peer>,
    pc: &'a config::PeerConfig,
}

impl<'a> ConfigBuilder<'a> {
    pub fn new(pc: &'a config::PeerConfig) -> Self {
        ConfigBuilder {
            peers: HashMap::new(),
            pc,
        }
    }

    pub fn build(self) -> Config {
        Config { peers: self.peers }
    }

    fn insert_with<'b>(
        &'b mut self,
        err: &mut Vec<ConfigError>,
        s: &config::Source,
        p: &proto::Peer,
        update: impl for<'c> FnOnce(&'c mut Peer) -> (),
    ) -> &'b mut Peer {
        match self.peers.entry(p.public_key.clone()) {
            hash_map::Entry::Occupied(ent) => {
                err.push(ConfigError::new("Duplicate public key", s, p, true));
                ent.into_mut()
            }
            hash_map::Entry::Vacant(ent) => {
                let ent = ent.insert(Peer {
                    endpoint: None,
                    psk: None,
                    keepalive: 0,
                    ipv4: vec![],
                    ipv6: vec![],
                });
                update(ent);
                ent
            }
        }
    }

    fn add_peer(err: &mut Vec<ConfigError>, ent: &mut Peer, s: &config::Source, p: &proto::Peer) {
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
            let msg = if added {
                "Some IPs removed"
            } else {
                "All IPs removed"
            };
            err.push(ConfigError::new(msg, s, p, !added));
        }
    }

    pub fn add_server(
        &mut self,
        err: &mut Vec<ConfigError>,
        s: &config::Source,
        p: &proto::Server,
    ) {
        if !valid_key(&p.peer.public_key) {
            err.push(ConfigError::new("Invalid public key", s, &p.peer, true));
            return;
        }

        if p.peer.public_key == self.pc.own_public_key {
            return;
        }

        let pc = self.pc;
        let ent = self.insert_with(err, s, &p.peer, |ent| {
            ent.psk = s.psk.clone();
            ent.endpoint = Some(p.endpoint.clone());
            ent.keepalive = pc.fix_keepalive(p.keepalive);
        });

        Self::add_peer(err, ent, s, &p.peer)
    }

    pub fn add_road_warrior(
        &mut self,
        err: &mut Vec<ConfigError>,
        s: &config::Source,
        p: &proto::RoadWarrior,
    ) {
        if !valid_key(&p.peer.public_key) {
            err.push(ConfigError::new("Invalid public key", s, &p.peer, true));
            return;
        }

        let ent = if p.base == self.pc.own_public_key {
            self.insert_with(err, s, &p.peer, |_| {})
        } else {
            match self.peers.get_mut(&p.base) {
                Some(ent) => ent,
                None => {
                    err.push(ConfigError::new("Unknown base peer", s, &p.peer, true));
                    return;
                }
            }
        };
        Self::add_peer(err, ent, s, &p.peer)
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
            let old_endpoint;
            if let Some(old_peer) = old.peers.get(pubkey) {
                if *old_peer == *conf {
                    continue;
                }
                old_endpoint = old_peer.endpoint.clone();
            } else {
                old_endpoint = None;
            }

            proc.arg("peer");
            proc.arg(pubkey);

            if old_endpoint != conf.endpoint {
                if let Some(ref endpoint) = conf.endpoint {
                    proc.arg("endpoint");
                    proc.arg(format!("{}", endpoint));
                }
            }

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
