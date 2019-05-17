// Copyright 2019 Hristo Venev
//
// See COPYING.

#![deny(rust_2018_idioms)]

#[macro_use]
extern crate arrayref;

use std::ffi::{OsStr, OsString};
use std::time::Instant;
use std::{env, process, thread};

#[cfg(feature = "toml")]
use std::{fs, io};
#[cfg(feature = "toml")]
use toml;

mod builder;
mod config;
mod manager;
mod model;
mod proto;
mod wg;

#[cfg(feature = "toml")]
fn file_config(path: OsString) -> io::Result<config::Config> {
    let mut data = String::new();
    {
        use io::Read;
        let mut config_file = fs::File::open(path)?;
        config_file.read_to_string(&mut data)?;
    }
    let mut de = toml::Deserializer::new(&data);
    serde::Deserialize::deserialize(&mut de)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn cli_config(args: &mut impl Iterator<Item = OsString>) -> Option<config::Config> {
    use std::str::FromStr;

    let mut cfg = config::Config::default();

    let mut cur_src: Option<&mut config::Source> = None;
    while let Some(key) = args.next() {
        let arg;

        if let Some(ref mut s) = cur_src {
            if key == "psk" {
                arg = args.next()?;
                let arg = arg.to_str()?;
                s.psk = Some(model::Key::from_str(arg).ok()?);
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
        }
        cur_src = None;

        if key == "min_keepalive" {
            arg = args.next()?;
            let arg = arg.to_str()?;
            cfg.peer_config.min_keepalive = u32::from_str(arg).ok()?;
            continue;
        }
        if key == "max_keepalive" {
            arg = args.next()?;
            let arg = arg.to_str()?;
            cfg.peer_config.max_keepalive = u32::from_str(arg).ok()?;
            continue;
        }
        if key == "refresh_sec" {
            arg = args.next()?;
            let arg = arg.to_str()?;
            cfg.update_config.refresh_sec = u32::from_str(arg).ok()?;
            continue;
        }
        if key == "source" {
            let name = args.next()?.into_string().ok()?;
            let url = args.next()?.into_string().ok()?;
            cur_src = Some(cfg.sources.entry(name).or_insert(config::Source {
                url,
                psk: None,
                ipv4: model::Ipv4Set::new(),
                ipv6: model::Ipv6Set::new(),
                required: false,
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

fn help(argv0: &str, _args: &mut impl Iterator<Item = OsString>) -> i32 {
    print!(
        "\
Usage:
    {} IFNAME CONFIG         - run daemon on iterface
    {} --check-source PATH   - validate source JSON
    {} --cmdline IFNAME ...  - run daemon using config passed as arguments
",
        argv0, argv0, argv0
    );
    1
}

fn maybe_get_var(out: &mut Option<impl From<OsString>>, var: impl AsRef<OsStr>) {
    let var = var.as_ref();
    if let Some(s) = env::var_os(var) {
        env::remove_var(var);
        *out = Some(s.into());
    }
}

#[cfg(feature = "toml")]
fn run_with_file(argv0: &str, args: &mut impl Iterator<Item = OsString>) -> i32 {
    let ifname = match args.next() {
        Some(v) => v,
        None => return usage(argv0),
    };
    let path = match args.next() {
        Some(v) => v,
        None => return usage(argv0),
    };
    if args.next().is_some() {
        return usage(argv0);
    }

    let config = match file_config(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("<1>Failed to load config: {}", e);
            return 1;
        }
    };
    run_daemon(ifname, config)
}

#[cfg(not(feature = "toml"))]
fn run_with_file(_: &str, _: &mut impl Iterator<Item = OsString>) -> i32 {
    eprintln!("<1>Config loading not supported");
    1
}

fn run_with_cmdline(argv0: &str, args: &mut impl Iterator<Item = OsString>) -> i32 {
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
    maybe_get_var(&mut config.cache_directory, "CACHE_DIRECTORY");
    maybe_get_var(&mut config.runtime_directory, "RUNTIME_DIRECTORY");

    let mut m = match manager::Manager::new(ifname, config) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("<1>Failed to open device: {}", e);
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

fn run_check_source(argv0: &str, args: &mut impl Iterator<Item = OsString>) -> i32 {
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
    let mut run: for<'a> fn(&'a str, &'a mut std::vec::IntoIter<OsString>) -> i32 = run_with_file;
    let mut parse_args = true;
    for arg in iter_args {
        if !parse_args || !arg.to_string_lossy().starts_with('-') {
            args.push(arg);
        } else if arg == "--" {
            parse_args = false;
        } else if arg == "-h" || arg == "--help" {
            run = help;
            break;
        } else if arg == "--check-source" {
            run = run_check_source;
            parse_args = false;
        } else if arg == "--cmdline" {
            run = run_with_cmdline;
            parse_args = false;
        } else {
            usage(&argv0);
        }
    }

    let mut args = args.into_iter();
    process::exit(run(&argv0, &mut args));
}
