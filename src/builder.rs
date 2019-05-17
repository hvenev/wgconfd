// Copyright 2019 Hristo Venev
//
// See COPYING.

use crate::{config, model, proto};
use std::collections::hash_map;
use std::{error, fmt};

#[derive(Debug)]
pub struct ConfigError {
    pub url: String,
    pub peer: model::Key,
    pub important: bool,
    err: &'static str,
}

impl ConfigError {
    fn new(err: &'static str, s: &config::Source, p: &proto::Peer, important: bool) -> Self {
        Self {
            url: s.url.clone(),
            peer: p.public_key.clone(),
            important,
            err,
        }
    }
}

impl error::Error for ConfigError {}
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

pub struct ConfigBuilder<'a> {
    c: model::Config,
    err: Vec<ConfigError>,
    public_key: model::Key,
    pc: &'a config::PeerConfig,
}

impl<'a> ConfigBuilder<'a> {
    #[inline]
    pub fn new(public_key: model::Key, pc: &'a config::PeerConfig) -> Self {
        Self {
            c: model::Config::default(),
            err: vec![],
            public_key,
            pc,
        }
    }

    #[inline]
    pub fn build(self) -> (model::Config, Vec<ConfigError>) {
        (self.c, self.err)
    }

    #[inline]
    pub fn add_server(&mut self, s: &config::Source, p: &proto::Server) {
        if p.peer.public_key == self.public_key {
            return;
        }

        let pc = self.pc;
        let ent = insert_peer(&mut self.c, &mut self.err, s, &p.peer, |ent| {
            ent.psk = s.psk.clone();
            ent.endpoint = Some(p.endpoint.clone());
            ent.keepalive = pc.fix_keepalive(p.keepalive);
        });

        add_peer(&mut self.err, ent, s, &p.peer)
    }

    #[inline]
    pub fn add_road_warrior(&mut self, s: &config::Source, p: &proto::RoadWarrior) {
        if p.peer.public_key == self.public_key {
            self.err.push(ConfigError::new(
                "The local peer cannot be a road warrior",
                s,
                &p.peer,
                true,
            ));
            return;
        }

        let ent = if p.base == self.public_key {
            insert_peer(&mut self.c, &mut self.err, s, &p.peer, |_| {})
        } else if let Some(ent) = self.c.peers.get_mut(&p.base) {
            ent
        } else {
            self.err
                .push(ConfigError::new("Unknown base peer", s, &p.peer, true));
            return;
        };
        add_peer(&mut self.err, ent, s, &p.peer)
    }
}

#[inline]
fn insert_peer<'b>(
    c: &'b mut model::Config,
    err: &mut Vec<ConfigError>,
    s: &config::Source,
    p: &proto::Peer,
    update: impl for<'c> FnOnce(&'c mut model::Peer) -> (),
) -> &'b mut model::Peer {
    match c.peers.entry(p.public_key.clone()) {
        hash_map::Entry::Occupied(ent) => {
            err.push(ConfigError::new("Duplicate public key", s, p, true));
            ent.into_mut()
        }
        hash_map::Entry::Vacant(ent) => {
            let ent = ent.insert(model::Peer {
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

fn add_peer(
    err: &mut Vec<ConfigError>,
    ent: &mut model::Peer,
    s: &config::Source,
    p: &proto::Peer,
) {
    let mut added = false;
    let mut removed = false;

    for i in &p.ipv4 {
        if s.ipv4.contains(i) {
            ent.ipv4.push(*i);
            added = true;
        } else {
            removed = true;
        }
    }
    for i in &p.ipv6 {
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
