// Copyright © 2018 Cormac O'Brien
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

pub mod bsp;
pub mod console;
pub mod engine;
pub mod host;
pub mod math;
pub mod mdl;
pub mod model;
pub mod net;
pub mod pak;
pub mod parse;
pub mod sprite;
pub mod util;
pub mod vfs;
pub mod wad;

pub static DEFAULT_BASEDIR: &'static str = "id1";
pub const MAX_LIGHTSTYLES: usize = 64;

/// The maximum number of `.pak` files that should be loaded at runtime.
///
/// The original engine does not make this restriction, and this limit can be increased if need be.
pub const MAX_PAKFILES: usize = 32;
