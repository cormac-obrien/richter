# Richter
An open-source implementation of the Quake engine in Rust

![alt tag](https://i.imgur.com/zTDrWzm.png)

## Building

Richter makes use of feature gates and compiler plugins, which means you'll need a nightly build of
`rustc`.

If you want to test the renderer, you'll want to build the release version:

    $ cargo run --release

## Legal

This software is released under the terms of the MIT License (see LICENSE.txt).

This project is in no way affiliated with id Software.

Due to licensing restrictions, the data files necessary to run Quake cannot be distributed with this
package. These files can be retrieved from id's FTP server at `ftp://ftp.idsoftware.com/idstuff/quake`.
