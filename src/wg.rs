// SPDX-License-Identifier: LGPL-3.0-or-later
//
// Copyright 2019 Hristo Venev

use crate::{fileutil, model};
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::{env, fmt, io};

pub struct Device {
    ifname: OsString,
    tmpdir: PathBuf,
}

impl Device {
    #[inline]
    pub fn open(ifname: OsString, tmpdir: PathBuf) -> io::Result<Self> {
        let dev = Self { ifname, tmpdir };
        let _ = dev.get_public_key()?;
        Ok(dev)
    }

    fn wg_command() -> Command {
        let wg = match env::var_os("WG") {
            None => OsString::new(),
            Some(v) => v,
        };

        Command::new(if wg.is_empty() {
            OsStr::new("wg")
        } else {
            wg.as_os_str()
        })
    }

    pub fn get_public_key(&self) -> io::Result<model::Key> {
        let mut proc = Self::wg_command();
        proc.stdin(Stdio::null());
        proc.stdout(Stdio::piped());
        proc.arg("show");
        proc.arg(&self.ifname);
        proc.arg("public-key");

        let r = proc.output()?;
        if !r.status.success() {
            return Err(io::Error::new(io::ErrorKind::Other, "child process failed"));
        }

        let mut out = r.stdout;
        if out.last().copied() == Some(b'\n') {
            out.pop();
        }
        model::Key::from_base64(&out)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid public key"))
    }

    pub fn apply_diff(&mut self, old: &model::Config, new: &model::Config) -> io::Result<()> {
        let mut config = String::new();

        for (pubkey, conf) in &new.peers {
            let old_endpoint;
            let old_psk;
            if let Some(old_peer) = old.peers.get(pubkey) {
                if *old_peer == *conf {
                    continue;
                }
                old_endpoint = old_peer.endpoint;
                old_psk = old_peer.psk.as_ref();
            } else {
                old_endpoint = None;
                old_psk = None;
            }

            use fmt::Write;
            write!(
                config,
                "[Peer]\nPublicKey={}\nPersistentKeepalive={}\nAllowedIPs",
                pubkey, conf.keepalive
            )
            .unwrap();
            let mut delim = '=';
            for ip in &conf.ipv4 {
                config.push(delim);
                delim = ',';
                write!(config, "{}", ip).unwrap();
            }
            for ip in &conf.ipv6 {
                config.push(delim);
                delim = ',';
                write!(config, "{}", ip).unwrap();
            }
            config.push('\n');

            if old_endpoint != conf.endpoint {
                if let Some(ref endpoint) = conf.endpoint {
                    write!(config, "Endpoint={}\n", endpoint).unwrap();
                }
            }

            if old_psk != conf.psk.as_ref() {
                config.push_str("PresharedKey=");
                if let Some(psk) = conf.psk.as_ref() {
                    writeln!(config, "{}", psk).unwrap();
                    config.push('\n');
                } else {
                    config.push_str("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\n");
                }
            }
        }

        {
            let mut config_file = fileutil::Writer::new_in(&self.tmpdir)?;
            io::Write::write_all(config_file.file(), config.as_bytes())?;
            let config_file = config_file.done();

            let mut proc = Self::wg_command();
            proc.stdin(Stdio::null());
            proc.stdout(Stdio::null());
            proc.arg("addconf");
            proc.arg(&self.ifname);
            proc.arg(config_file.path());

            let r = proc.status()?;
            if !r.success() {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "`wg setconf' process failed",
                ));
            }
        }

        let mut proc = Self::wg_command();
        let mut any_removed = false;
        proc.stdin(Stdio::null());
        proc.stdout(Stdio::null());
        proc.arg("set");
        proc.arg(&self.ifname);

        for pubkey in old.peers.keys() {
            if new.peers.contains_key(pubkey) {
                continue;
            }
            any_removed = true;
            proc.arg("peer");
            proc.arg(pubkey.to_string());
            proc.arg("remove");
        }

        if any_removed {
            let r = proc.status()?;
            if !r.success() {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "`wg set' process failed",
                ));
            }
        }

        Ok(())
    }
}
