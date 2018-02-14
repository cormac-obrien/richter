// Copyright © 2017 Cormac O'Brien
//
// Permission is hereby granted, free of charge, to any person obtaining a copy of this software
// and associated documentation files (the "Software"), to deal in the Software without
// restriction, including without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all copies or
// substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING
// BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
// NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
// DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

extern crate docopt;
extern crate richter;
#[macro_use]
extern crate serde_derive;

use std::env;
use std::fs;
use std::fs::File;
use std::io::BufWriter;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::exit;

use richter::common::pak::Pak;
use richter::common::pak::PakError;

use docopt::Docopt;

#[derive(Deserialize)]
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
Copyright © 2018 Cormac O'Brien
Released under the terms of the MIT License
";

fn main() {
    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
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
