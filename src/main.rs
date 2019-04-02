// Copyright 2019 Hristo Venev
//
// See COPYING.

#[macro_use]
extern crate arrayref;

use std::{env, fs, io, process, thread};
use std::time::Instant;
use std::ffi::{OsStr, OsString};
use toml;

mod builder;
mod model;
mod config;
mod proto;
mod wg;
mod manager;

fn load_config(path: &OsStr) -> io::Result<config::Config> {
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

fn usage(argv0: &str) -> i32 {
    eprintln!("<1>Invalid arguments. See `{} --help` for more information", argv0);
    1
}

fn help(argv0: &str) -> i32 {
    println!("Usage:");
    println!("    {} IFNAME CONFIG         - run daemon on iterface", argv0);
    println!("    {} --check-source PATH   - validate source JSON", argv0);
    1
}

fn maybe_get_var(out: &mut Option<impl From<OsString>>, var: impl AsRef<OsStr>) {
    let var = var.as_ref();
    if let Some(s) = env::var_os(var) {
        env::remove_var(var);
        *out = Some(s.into());
    }
}

fn run_daemon(argv0: String, args: Vec<OsString>) -> i32 {
    if args.len() != 2 {
        return usage(&argv0);
    }
    let mut args = args.into_iter();
    let ifname = args.next().unwrap();
    let config_path = args.next().unwrap();
    assert!(args.next().is_none());

    let mut config = match load_config(&config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("<1>Failed to load config: {}", e);
            process::exit(1);
        }
    };

    maybe_get_var(&mut config.cache_directory, "CACHE_DIRECTORY");
    maybe_get_var(&mut config.runtime_directory, "RUNTIME_DIRECTORY");

    let mut m = match manager::Manager::new(ifname, config) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("<1>Failed to open device: {}", e);
            process::exit(1);
        }
    };

    loop {
        let tm = match m.update() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("<1>{}", e);
                process::exit(1);
            }
        };
        let now = Instant::now();
        if tm > now {
            let sleep = tm.duration_since(now);
            thread::sleep(sleep);
        }
    }
}

fn run_check_source(argv0: String, args: Vec<OsString>) -> i32 {
    if args.len() != 1 {
        usage(&argv0);
    }
    let mut args = args.into_iter();
    let path = args.next().unwrap();
    assert!(args.next().is_none());

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

fn main() -> () {
    let mut iter_args = env::args_os();
    let argv0 = iter_args.next().unwrap().to_string_lossy().into_owned();

    let mut args = Vec::new();
    let mut run: for<'a> fn(String, Vec<OsString>) -> i32 = run_daemon;
    let mut parse_args = true;
    for arg in iter_args {
        if !parse_args || !arg.to_string_lossy().starts_with('-') {
            args.push(arg);
        } else if arg == "--" {
            parse_args = false;
        } else if arg == "-h" || arg == "--help" {
            process::exit(help(&argv0));
        } else if arg == "--check-source" {
            run = run_check_source;
            parse_args = false;
        } else {
            usage(&argv0);
        }
    }

    process::exit(run(argv0, args));
}
