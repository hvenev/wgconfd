// SPDX-License-Identifier: LGPL-3.0-or-later
//
// Copyright 2019 Hristo Venev

use super::Source;
use crate::{config, model, proto};
use std::collections::hash_map;
use std::{error, fmt};

#[derive(Debug)]
pub struct Error {
    pub src: String,
    pub peer: model::Key,
    important: bool,
    err: &'static str,
}

impl Error {
    fn new(err: &'static str, src: &Source, p: &proto::Peer, important: bool) -> Self {
        Self {
            src: src.config.name.clone(),
            peer: p.public_key,
            important,
            err,
        }
    }

    #[inline]
    pub fn important(&self) -> bool {
        self.important
    }
}

impl error::Error for Error {}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} [{}]/[{}]: {}",
            if self.important {
                "invalid peer"
            } else {
                "misconfigured peer"
            },
            self.src,
            self.peer,
            self.err
        )
    }
}

struct PeerContact<'a> {
    endpoint: Option<model::Endpoint>,
    psk: Option<&'a model::Secret>,
    keepalive: u32,
}

pub(super) struct ConfigBuilder<'a> {
    c: model::Config,
    err: Vec<Error>,
    public_key: model::Key,
    gc: &'a config::GlobalConfig,
}

impl<'a> ConfigBuilder<'a> {
    #[inline]
    pub fn new(public_key: model::Key, gc: &'a config::GlobalConfig) -> Self {
        Self {
            c: model::Config::empty(),
            err: vec![],
            public_key,
            gc,
        }
    }

    #[inline]
    pub fn build(self) -> (model::Config, Vec<Error>) {
        (self.c, self.err)
    }

    #[inline]
    pub fn add_server(&mut self, src: &Source, p: &proto::Server) {
        let gc = self.gc;

        let mut contact = match peer_contact(gc, src, &p.peer) {
            Ok(v) => v,
            Err(e) => {
                self.err.push(e);
                return;
            }
        };
        if contact.endpoint.is_none() {
            contact.endpoint = Some(p.endpoint);
        }

        if p.peer.public_key == self.public_key {
            return;
        }

        let ent = insert_peer(&mut self.c, &mut self.err, src, &p.peer, contact);
        add_peer(&mut self.err, ent, src, &p.peer)
    }

    #[inline]
    pub fn add_road_warrior(&mut self, src: &Source, p: &proto::RoadWarrior) {
        let contact = match peer_contact(self.gc, src, &p.peer) {
            Ok(v) => v,
            Err(e) => {
                self.err.push(e);
                return;
            }
        };

        if p.peer.public_key == self.public_key {
            self.err.push(Error::new(
                "the local peer cannot be a road warrior",
                src,
                &p.peer,
                true,
            ));
            return;
        }

        let ent = if p.base == self.public_key {
            if !src.config.allow_road_warriors {
                self.err.push(Error::new(
                    "road warriors from this source not allowed",
                    src,
                    &p.peer,
                    true,
                ));
                return;
            }
            insert_peer(&mut self.c, &mut self.err, src, &p.peer, contact)
        } else if let Some(ent) = self.c.peers.get_mut(&p.base) {
            ent
        } else {
            self.err
                .push(Error::new("unknown base peer", src, &p.peer, true));
            return;
        };
        add_peer(&mut self.err, ent, src, &p.peer)
    }
}

#[inline]
fn insert_peer<'b>(
    c: &'b mut model::Config,
    err: &mut Vec<Error>,
    src: &Source,
    p: &proto::Peer,
    contact: PeerContact<'_>,
) -> &'b mut model::Peer {
    match c.peers.entry(p.public_key) {
        hash_map::Entry::Occupied(ent) => {
            err.push(Error::new("duplicate public key", src, p, true));
            ent.into_mut()
        }
        hash_map::Entry::Vacant(ent) => ent.insert(model::Peer {
            endpoint: contact.endpoint,
            psk: contact.psk.cloned(),
            keepalive: contact.keepalive,
            ipv4: vec![],
            ipv6: vec![],
        }),
    }
}

fn peer_contact<'a>(
    gc: &'a config::GlobalConfig,
    src: &'a Source,
    p: &proto::Peer,
) -> Result<PeerContact<'a>, Error> {
    let mut r = PeerContact {
        psk: src.config.psk.as_ref(),
        endpoint: None,
        keepalive: gc.fix_keepalive(p.keepalive),
    };

    if let Some(pc) = gc.peers.get(&p.public_key) {
        if let Some(ref want_src) = &pc.source {
            if *want_src != src.config.name {
                return Err(Error::new("peer source not allowed", src, p, true));
            }
        }

        if let Some(endpoint) = pc.endpoint {
            r.endpoint = Some(endpoint);
        }

        if let Some(ref psk) = &pc.psk {
            r.psk = Some(psk);
        }

        if let Some(keepalive) = pc.keepalive {
            r.keepalive = keepalive;
        }
    }

    Ok(r)
}

fn add_peer(err: &mut Vec<Error>, ent: &mut model::Peer, src: &Source, p: &proto::Peer) {
    let mut added = false;
    let mut removed = false;

    for i in &p.ipv4 {
        if src.config.ipv4.contains(i) {
            ent.ipv4.push(*i);
            added = true;
        } else {
            removed = true;
        }
    }
    for i in &p.ipv6 {
        if src.config.ipv6.contains(i) {
            ent.ipv6.push(*i);
            added = true;
        } else {
            removed = true;
        }
    }

    if removed {
        let msg = if added {
            "some IPs removed"
        } else {
            "all IPs removed"
        };
        err.push(Error::new(msg, src, p, !added));
    }
}
