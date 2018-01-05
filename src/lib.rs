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

#![feature(custom_derive)]
#![feature(try_from)]

extern crate arrayvec;
#[macro_use]
extern crate bitflags;
extern crate byteorder;
extern crate cgmath;
extern crate chrono;
extern crate env_logger;
extern crate glutin;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
#[macro_use]
extern crate nom;
extern crate num;
#[macro_use]
extern crate num_derive;
extern crate rand;
extern crate regex;
extern crate rodio;
extern crate time;
extern crate winit;

pub mod bsp;
pub mod client;
pub mod console;
pub mod engine;
pub mod event;
pub mod input;
pub mod lump;
pub mod math;
pub mod mdl;
pub mod model;
pub mod net;
pub mod pak;
pub mod parse;
pub mod progs;
// pub mod qw;
pub mod sprite;
pub mod server;
pub mod util;
pub mod world;
