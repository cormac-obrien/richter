# Richter
An open-source implementation of the Quake engine in Rust

![alt tag](https://i.imgur.com/zTDrWzm.png)

## Status

Richter is currently in pre-alpha development. This means things will break
frequently, documentation may be sparse, and many features will be missing.

## Building

Richter makes use of feature gates and compiler plugins, which means you'll need
a nightly build of `rustc`.

Because a Quake distribution contains multiple binaries, this software is
packaged as a Cargo library project. The source files for binaries are located
in the `src/bin` directory and can be run with

    $ cargo run --bin <name>

where `<name>` is the name of the source file without the `.rs` extension.

## Legal

This software is released under the terms of the MIT License (see LICENSE.txt).

This project is in no way affiliated with id Software.

Due to licensing restrictions, the data files necessary to run Quake cannot be distributed with this
package. These files can be retrieved from id's FTP server at `ftp://ftp.idsoftware.com/idstuff/quake`.
