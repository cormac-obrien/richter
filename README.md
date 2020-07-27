# Richter

[![Build Status](https://travis-ci.org/cormac-obrien/richter.svg?branch=devel)](https://travis-ci.org/cormac-obrien/richter)

A modern implementation of the Quake engine in Rust.

![alt tag](https://i.imgur.com/25nOENn.png)

## Status

Richter is in pre-alpha development, so it's still under heavy construction.
However, the client is nearly alpha-ready -- check out the Client section below to see progress.

### Client

The client is capable of connecting to and playing on original Quake servers using `sv_protocol 15`.
To connect to a Quake server, run

```
$ cargo run --release --bin quake-client -- --connect <server_ip>:<server_port>
```

Quake servers run on port 26000 by default.
I can guarantee compatibility with FitzQuake and its derived engines, as I use the QuakeSpasm server for development (just remember `sv_protocol 15`).

The client also supports demo playback using the `--demo` option:

```
$ cargo run --release --bin quake-client -- --demo <demo_file>
```

This works for demos in the PAK archives (e.g. `demo1.dem`) or any demos you happen to have placed in the `id1` directory.

#### Feature checklist

- Networking
  - [x] NetQuake network protocol implementation (`sv_protocol 15`)
    - [x] Connection protocol implemented
    - [x] All in-game server commands handled
    - [x] Carryover between levels
  - [ ] FitzQuake extended protocol support (`sv_protocol 666`)
- Rendering
  - [x] Deferred dynamic lighting
  - [x] Particle effects
  - Brush model (`.bsp`) rendering
    - Textures
      - [x] Static textures
      - [x] Animated textures
      - [x] Alternate animated textures
      - [x] Liquid texture warping
      - [ ] Sky texture scrolling (currently partial support)
    - [x] Lightmaps
    - [x] Occlusion culling
  - Alias model (`.mdl`) rendering
    - [x] Keyframe animation
      - [x] Static keyframes
      - [x] Animated keyframes
    - [ ] Keyframe interpolation
    - [ ] Ambient lighting
    - [ ] Viewmodel rendering
  - UI
    - [x] Console
    - [x] HUD
    - [x] Level intermissions
    - [ ] On-screen messages
    - [ ] Menus
- Sound
  - [x] Loading and playback
  - [x] Entity sound
  - [ ] Ambient sound
  - [x] Spatial attenuation
  - [ ] Stereo spatialization
- Console
  - [x] Line editing
  - [x] History browsing
  - [x] Cvar modification
  - [x] Command execution
  - [x] Quake script file execution
- Demos
  - [x] Demo playback
  - [ ] Demo recording
- File formats
  - [x] BSP loader
  - [x] MDL loader
  - [x] SPR loader
  - [x] PAK archive extraction
  - [x] WAD archive extraction

### Server

The Richter server is still in its early stages, so there's no checklist here yet.
However, you can still check out the QuakeC bytecode VM in the [`progs` module](https://github.com/cormac-obrien/richter/blob/devel/src/server/progs/mod.rs).

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
