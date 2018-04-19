# Richter
An open-source implementation of the Quake engine in Rust

![alt tag](https://i.imgur.com/O0KUuBp.jpg)

## Status

Richter is currently in pre-alpha development. This means that the engine architecture is still
being designed. The most development time is currently being focused on the client. Neither the
client nor the server is in a "working" state yet.

## Building

Richter makes use of feature gates and compiler plugins, which means you'll need a nightly build of
`rustc`. The simplest way to do this is to download [rustup](https://www.rustup.rs/) and follow the
directions.

Because a Quake distribution contains multiple binaries, this software is packaged as a Cargo
library project. The source files for binaries are located in the `src/bin` directory and can be run
with

    $ cargo run --bin <name>

where `<name>` is the name of the source file without the `.rs` extension.

## Legal

This software is released under the terms of the MIT License (see LICENSE.txt).

This project is in no way affiliated with id Software LLC, Bethesda Softworks LLC, or ZeniMax Media
Inc. Information regarding the Quake trademark can be found at Bethesda's [legal information
page](https://bethesda.net/en/document/legal-information).

Due to licensing restrictions, the data files necessary to run Quake cannot be distributed with this
package. `pak0.pak`, which contains the files for the first episode ("shareware Quake"), can be
retrieved from id's FTP server at `ftp://ftp.idsoftware.com/idstuff/quake`. The full game can be
purchased from a number of retailers including Steam and GOG.
