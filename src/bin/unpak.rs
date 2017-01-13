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
use std::path::Path;
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
    -h, --help     Show this message.
    -v, --verbose  Produce detailed output.
        --version  Print version information.
";

fn main() {
    let args: Args = Docopt::new(USAGE)
                         .and_then(|d| d.decode())
                         .unwrap_or_else(|e| e.exit());

    let pak = match Pak::load(&args.arg_source) {
        Ok(p) => p,
        Err(why) => {
            println!("Couldn't open {}: {}", &args.arg_source, why);
            exit(1);
        }
    };

    for (k, v) in pak.iter() {
        let path = Path::new(k);

        if let Some(p) = path.parent() {
            if !p.exists() {
                if let Err(why) = fs::create_dir_all(p) {
                    println!("Couldn't create parent directories: {}", why);
                    exit(1);
                }
            }
        }

        let mut f = match File::create(k) {
            Ok(f) => f,
            Err(why) => {
                println!("Couldn't open {}: {}", k, why);
                exit(1);
            }
        };

        let mut writer = BufWriter::new(f);
        writer.write_all(v);
    }
}
