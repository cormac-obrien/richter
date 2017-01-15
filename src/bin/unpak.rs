#![feature(plugin)]
#![plugin(docopt_macros)]

extern crate docopt;
extern crate richter;
extern crate rustc_serialize;

use docopt::Docopt;
use richter::pak::{Pak, PakError};
use std::env;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::exit;

#[derive(RustcDecodable)]
struct Args {
    arg_source: String,
    arg_dest: Option<String>,
    flag_h: bool,
    flag_help: bool,
    flag_v: bool,
    flag_verbose: bool,
    flag_version: bool,
}

const USAGE: &'static str = "
Usage: unpak <source>
       unpak <source> <dest>

Options:
    -v, --verbose  Produce detailed output.

    -h, --help     Show this message and exit.
        --version  Print version information and exit.
";

const VERSION: &'static str = "
unpak 0.1
Copyright © 2016 Cormac O'Brien
Released under the terms of the MIT License
";

fn main() {
    let args: Args = Docopt::new(USAGE)
                         .and_then(|d| d.decode())
                         .unwrap_or_else(|e| e.exit());

    if args.flag_help || args.flag_h {
        println!("{}", USAGE);
        exit(0);
    }

    if args.flag_version {
        println!("{}", VERSION);
        exit(0);
    }

    let mut pak = Pak::new();
    match pak.add(&args.arg_source) {
        Ok(p) => p,
        Err(why) => {
            println!("Couldn't open {}: {}", &args.arg_source, why);
            exit(1);
        }
    };

    for (k, v) in pak.iter() {
        let mut path = PathBuf::new();

        if let Some(ref d) = args.arg_dest {
            path.push(d);
        }

        path.push(k);

        if let Some(p) = path.parent() {
            if !p.exists() {
                if let Err(why) = fs::create_dir_all(p) {
                    println!("Couldn't create parent directories: {}", why);
                    exit(1);
                }
            }
        }

        let file = match File::create(&path) {
            Ok(f) => f,
            Err(why) => {
                println!("Couldn't open {}: {}", path.to_str().unwrap(), why);
                exit(1);
            }
        };

        let mut writer = BufWriter::new(file);
        match writer.write_all(v) {
            Ok(_) => (),
            Err(why) => {
                println!("Couldn't write to {}: {}", path.to_str().unwrap(), why);
                exit(1);
            }
        }
    }
}
