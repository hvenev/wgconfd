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
            src: src.name.clone(),
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

        let psk = match find_psk(gc, src, &p.peer) {
            Ok(v) => v,
            Err(e) => {
                self.err.push(e);
                return;
            }
        };

        if p.peer.public_key == self.public_key {
            return;
        }

        let ent = insert_peer(&mut self.c, &mut self.err, src, &p.peer, psk, |ent| {
            ent.endpoint = Some(p.endpoint);
            ent.keepalive = gc.fix_keepalive(p.keepalive);
        });

        add_peer(&mut self.err, ent, src, &p.peer)
    }

    #[inline]
    pub fn add_road_warrior(&mut self, src: &Source, p: &proto::RoadWarrior) {
        let psk = match find_psk(&self.gc, src, &p.peer) {
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
            insert_peer(&mut self.c, &mut self.err, src, &p.peer, psk, |_| {})
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
    psk: Option<&model::Secret>,
    update: impl for<'c> FnOnce(&'c mut model::Peer) -> (),
) -> &'b mut model::Peer {
    match c.peers.entry(p.public_key) {
        hash_map::Entry::Occupied(ent) => {
            err.push(Error::new("duplicate public key", src, p, true));
            ent.into_mut()
        }
        hash_map::Entry::Vacant(ent) => {
            let ent = ent.insert(model::Peer {
                endpoint: None,
                psk: psk.cloned(),
                keepalive: 0,
                ipv4: vec![],
                ipv6: vec![],
            });
            update(ent);
            ent
        }
    }
}

fn find_psk<'a>(
    gc: &'a config::GlobalConfig,
    src: &'a Source,
    p: &proto::Peer,
) -> Result<Option<&'a model::Secret>, Error> {
    let want = match gc.peers.get(&p.public_key) {
        Some(v) => v,
        None => return Ok(None),
    };

    if let Some(ref want_src) = &want.source {
        if *want_src != src.name {
            return Err(Error::new("peer source not allowed", src, p, true));
        }
    }

    Ok(want.psk.as_ref().or_else(|| src.config.psk.as_ref()))
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
