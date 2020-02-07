// SPDX-License-Identifier: LGPL-3.0-or-later
//
// Copyright 2019 Hristo Venev

use crate::model;
use std::ffi::{OsStr, OsString};
use std::process::{Command, Stdio};
use std::{env, fmt, io};

pub struct Device {
    ifname: OsString,
}

impl Device {
    #[inline]
    pub fn open(ifname: OsString) -> io::Result<Self> {
        let dev = Self { ifname };
        let _ = dev.get_public_key()?;
        Ok(dev)
    }

    pub fn wg_command() -> Command {
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
        let mut proc = Self::wg_command();
        proc.stdin(Stdio::piped());
        proc.stdout(Stdio::null());
        proc.arg("set");
        proc.arg(&self.ifname);
        let mut stdin = String::new();

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

            proc.arg("peer");
            proc.arg(pubkey.to_string());

            proc.arg("persistent-keepalive");
            proc.arg(conf.keepalive.to_string());

            if old_endpoint != conf.endpoint {
                if let Some(ref endpoint) = conf.endpoint {
                    proc.arg("endpoint");
                    proc.arg(endpoint.to_string());
                }
            }

            if old_psk != conf.psk.as_ref() {
                proc.arg("preshared-key");
                proc.arg("-");
                if let Some(psk) = conf.psk.as_ref() {
                    use fmt::Write;
                    writeln!(stdin, "{}", psk).unwrap();
                } else {
                    stdin.push('\n');
                }
            }

            let mut ips = String::new();
            {
                use std::fmt::Write;
                for ip in &conf.ipv4 {
                    if !ips.is_empty() {
                        ips.push(',');
                    }
                    write!(ips, "{}", ip).unwrap();
                }
                for ip in &conf.ipv6 {
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
            proc.arg(pubkey.to_string());
            proc.arg("remove");
        }

        let mut proc = proc.spawn()?;
        {
            use io::Write;
            proc.stdin.as_mut().unwrap().write_all(stdin.as_bytes())?;
        }

        let r = proc.wait()?;
        if !r.success() {
            return Err(io::Error::new(io::ErrorKind::Other, "child process failed"));
        }
        Ok(())
    }
}
