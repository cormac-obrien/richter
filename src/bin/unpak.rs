// Copyright © 2018 Cormac O'Brien
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

extern crate richter;

use std::{
    fs,
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    process,
};

use richter::common::pak::Pak;

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
struct Opt {
    #[structopt(short, long)]
    verbose: bool,

    #[structopt(long)]
    version: bool,

    #[structopt(name = "INPUT_PAK", parse(from_os_str))]
    input_pak: PathBuf,

    #[structopt(name = "OUTPUT_DIR", parse(from_os_str))]
    output_dir: Option<PathBuf>,
}

const VERSION: &'static str = "
unpak 0.1
Copyright © 2020 Cormac O'Brien
Released under the terms of the MIT License
";

fn main() {
    let opt = Opt::from_args();

    if opt.version {
        println!("{}", VERSION);
        process::exit(0);
    }

    let pak = match Pak::new(&opt.input_pak) {
        Ok(p) => p,
        Err(why) => {
            println!("Couldn't open {:#?}: {}", &opt.input_pak, why);
            process::exit(1);
        }
    };

    for (k, v) in pak.iter() {
        let mut path = PathBuf::new();

        if let Some(ref d) = opt.output_dir {
            path.push(d);
        }

        path.push(k);

        if let Some(p) = path.parent() {
            if !p.exists() {
                if let Err(why) = fs::create_dir_all(p) {
                    println!("Couldn't create parent directories: {}", why);
                    process::exit(1);
                }
            }
        }

        let file = match File::create(&path) {
            Ok(f) => f,
            Err(why) => {
                println!("Couldn't open {}: {}", path.to_str().unwrap(), why);
                process::exit(1);
            }
        };

        let mut writer = BufWriter::new(file);
        match writer.write_all(v.as_ref()) {
            Ok(_) => (),
            Err(why) => {
                println!("Couldn't write to {}: {}", path.to_str().unwrap(), why);
                process::exit(1);
            }
        }
    }
}
