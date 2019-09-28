// Copyright 2019 Hristo Venev
//
// See COPYING.

use crate::model;
use std::ffi::{OsStr, OsString};
use std::process::{Command, Stdio};
use std::{env, io};

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
            return Err(io::Error::new(io::ErrorKind::Other, "Child process failed"));
        }

        let mut out = r.stdout;
        if out.ends_with(b"\n") {
            out.remove(out.len() - 1);
        }
        model::Key::from_bytes(&out)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid public key"))
    }

    pub fn apply_diff(&mut self, old: &model::Config, new: &model::Config) -> io::Result<()> {
        let mut proc = Self::wg_command();
        proc.stdin(Stdio::piped());
        proc.arg("set");
        proc.arg(&self.ifname);

        let mut psks = String::new();

        for (pubkey, conf) in &new.peers {
            let old_endpoint;
            if let Some(old_peer) = old.peers.get(pubkey) {
                if *old_peer == *conf {
                    continue;
                }
                old_endpoint = old_peer.endpoint;
            } else {
                old_endpoint = None;
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

            if let Some(psk) = &conf.psk {
                proc.arg("preshared-key");
                proc.arg("/dev/stdin");
                {
                    use std::fmt::Write;
                    writeln!(&mut psks, "{}", psk).unwrap();
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
            use std::io::Write;
            let stdin = proc.stdin.as_mut().unwrap();
            write!(stdin, "{}", psks)?;
        }

        let r = proc.wait()?;
        if !r.success() {
            return Err(io::Error::new(io::ErrorKind::Other, "Child process failed"));
        }
        Ok(())
    }
}
