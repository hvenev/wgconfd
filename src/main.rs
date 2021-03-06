// SPDX-License-Identifier: LGPL-3.0-or-later
//
// Copyright 2019 Hristo Venev

#![deny(rust_2018_idioms)]

#[macro_use]
extern crate arrayref;

use std::ffi::{OsStr, OsString};
use std::time::Instant;
use std::{env, mem, process, thread};

mod config;
mod fileutil;
mod manager;
mod model;
mod proto;
mod wg;

fn cli_config(mut args: impl Iterator<Item = OsString>) -> Option<config::Config> {
    enum State<'a> {
        Source(&'a mut config::Source),
        Peer(&'a mut config::Peer),
        None,
    }

    use std::str::FromStr;

    let mut cfg = config::Config::default();

    let mut cur = State::None;
    while let Some(key) = args.next() {
        let arg;

        match cur {
            State::Source(ref mut s) => {
                if key == "psk" {
                    arg = args.next()?;
                    s.psk = model::Secret::from_file(&arg).ok()?;
                    continue;
                }
                if key == "ipv4" {
                    arg = args.next()?;
                    let arg = arg.to_str()?;
                    for arg in arg.split(',') {
                        s.ipv4.insert(model::Ipv4Net::from_str(arg).ok()?);
                    }
                    continue;
                }
                if key == "ipv6" {
                    arg = args.next()?;
                    let arg = arg.to_str()?;
                    for arg in arg.split(',') {
                        s.ipv6.insert(model::Ipv6Net::from_str(arg).ok()?);
                    }
                    continue;
                }
                if key == "required" {
                    s.required = true;
                    continue;
                }
                if key == "deny_road_warriors" {
                    s.allow_road_warriors = false;
                    continue;
                }
                if key == "allow_road_warriors" {
                    s.allow_road_warriors = true;
                    continue;
                }
            }
            State::Peer(ref mut p) => {
                if key == "source" {
                    p.source = Some(args.next()?.into_string().ok()?);
                    continue;
                }
                if key == "endpoint" {
                    arg = args.next()?;
                    let arg = arg.to_str()?;
                    p.endpoint = Some(model::Endpoint::from_str(arg).ok()?);
                    continue;
                }
                if key == "psk" {
                    arg = args.next()?;
                    p.psk = model::Secret::from_file(&arg).ok()?;
                    continue;
                }
                if key == "keepalive" {
                    arg = args.next()?;
                    let arg = arg.to_str()?;
                    p.keepalive = Some(u32::from_str(arg).ok()?);
                    continue;
                }
            }
            State::None => {}
        }
        cur = State::None;

        if key == "min_keepalive" {
            arg = args.next()?;
            let arg = arg.to_str()?;
            cfg.global.min_keepalive = u32::from_str(arg).ok()?;
            continue;
        }
        if key == "max_keepalive" {
            arg = args.next()?;
            let arg = arg.to_str()?;
            cfg.global.max_keepalive = u32::from_str(arg).ok()?;
            continue;
        }
        if key == "refresh_sec" {
            arg = args.next()?;
            let arg = arg.to_str()?;
            cfg.updater.refresh_sec = u32::from_str(arg).ok()?;
            continue;
        }
        if key == "source" {
            let name = args.next()?.into_string().ok()?;
            let url = args.next()?.into_string().ok()?;
            cfg.sources.push(config::Source {
                name,
                url,
                psk: None,
                ipv4: model::Ipv4Set::new(),
                ipv6: model::Ipv6Set::new(),
                required: false,
                allow_road_warriors: true,
            });
            cur = State::Source(cfg.sources.last_mut().unwrap());
            continue;
        }
        if key == "peer" {
            arg = args.next()?;
            let key = model::Key::from_str(arg.to_str()?).ok()?;
            cur = State::Peer(cfg.global.peers.entry(key).or_insert(config::Peer {
                source: None,
                endpoint: None,
                psk: None,
                keepalive: None,
            }));
            continue;
        }

        return None;
    }

    Some(cfg)
}

fn usage(argv0: &str) -> i32 {
    eprintln!(
        "<1>Invalid arguments. See `{} --help` for more information",
        argv0
    );
    1
}

fn help(argv0: &str, args: Vec<OsString>) -> i32 {
    mem::drop(args);
    print!(
        "\
Usage:
    {} IFNAME CONFIG         - run daemon on interface
    {} --cmdline IFNAME ...  - run daemon using config passed as arguments
    {} --check-source PATH   - validate source JSON
",
        argv0, argv0, argv0
    );
    1
}

fn run_usage(argv0: &str, _: Vec<OsString>) -> i32 {
    usage(argv0)
}

fn maybe_get_var(out: &mut Option<impl From<OsString>>, var: impl AsRef<OsStr>) {
    let var = var.as_ref();
    if let Some(s) = env::var_os(var) {
        env::remove_var(var);
        *out = Some(s.into());
    }
}

#[cfg(feature = "toml")]
fn run_with_file(argv0: &str, args: Vec<OsString>) -> i32 {
    let (ifname, path) = match (move || {
        let mut args = args.into_iter();
        let a = args.next()?;
        let b = args.next()?;
        if args.next().is_some() {
            return None;
        }
        Some((a, b))
    })() {
        Some(v) => v,
        None => return usage(argv0),
    };

    let data = fileutil::load(&path);
    mem::drop(path);
    let data = match data {
        Ok(v) => v,
        Err(e) => {
            eprintln!("<1>Failed to load config file: {}", e);
            return 1;
        }
    };

    let config = toml::from_slice(&data);
    mem::drop(data);
    let config = match config {
        Ok(v) => v,
        Err(e) => {
            eprintln!("<1>Failed to parse config: {}", e);
            return 1;
        }
    };

    run_daemon(ifname, config)
}

#[cfg(not(feature = "toml"))]
fn run_with_file(_argv0: &str, _args: Vec<OsString>) -> i32 {
    eprintln!("<1>Config loading not supported");
    1
}

fn run_with_cmdline(argv0: &str, args: Vec<OsString>) -> i32 {
    let mut args = args.into_iter();

    let ifname = match args.next() {
        Some(v) => v,
        None => return usage(argv0),
    };

    let config = match cli_config(args) {
        Some(c) => c,
        None => {
            eprintln!("<1>Invalid config");
            return 1;
        }
    };

    run_daemon(ifname, config)
}

fn run_daemon(ifname: OsString, mut config: config::Config) -> i32 {
    maybe_get_var(&mut config.updater.cache_directory, "CACHE_DIRECTORY");
    maybe_get_var(&mut config.runtime_directory, "RUNTIME_DIRECTORY");

    let mut m = match manager::Manager::new(ifname, config) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("<1>Failed to start: {}", e);
            return 1;
        }
    };

    loop {
        let tm = match m.update() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("<1>{}", e);
                return 1;
            }
        };
        let now = Instant::now();
        if tm > now {
            let sleep = tm.duration_since(now);
            thread::sleep(sleep);
        }
    }
}

fn run_check_source(argv0: &str, args: Vec<OsString>) -> i32 {
    let mut args = args.into_iter();
    let path = match args.next() {
        Some(v) => v,
        None => return usage(argv0),
    };
    if args.next().is_some() {
        return usage(argv0);
    }

    match manager::load_source(&path) {
        Ok(_) => {
            println!("OK");
            0
        }
        Err(e) => {
            println!("{}", e);
            1
        }
    }
}

fn main() {
    let mut iter_args = env::args_os();
    let argv0 = iter_args.next().unwrap();
    let argv0 = argv0.to_string_lossy();

    let mut args = Vec::new();
    let mut run: for<'a> fn(&'a str, Vec<OsString>) -> i32 = run_with_file;
    for arg in &mut iter_args {
        if !arg.to_string_lossy().starts_with('-') {
            args.push(arg);
        } else if arg == "--" {
            break;
        } else if arg == "-h" || arg == "--help" {
            run = help;
            break;
        } else if arg == "--check-source" {
            run = run_check_source;
            break;
        } else if arg == "--cmdline" {
            run = run_with_cmdline;
            break;
        } else {
            run = run_usage;
            break;
        }
    }
    args.extend(iter_args);

    process::exit(run(&argv0, args));
}
