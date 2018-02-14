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

use std::fs::File;
use std::io::Read;

use cgmath::Deg;
use cgmath::Vector3;
use chrono::Duration;

// TODO: the palette should be host-specific and loaded alongside pak0.pak (or the latest PAK with a
// palette.lmp)
lazy_static! {
    static ref PALETTE: [u8; 768] = {
        let mut _palette = [0; 768];
        let mut f = File::open("pak0.pak.d/gfx/palette.lmp").unwrap();
        match f.read(&mut _palette) {
            Err(why) => panic!("{}", why),
            Ok(768) => _palette,
            _ => panic!("Bad read on pak0/gfx/palette.lmp"),
        }
    };
}

pub fn indexed_to_rgba(indices: &[u8]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(4 * indices.len());
    for i in 0..indices.len() {
        if indices[i] != 0xFF {
            for c in 0..3 {
                rgba.push(PALETTE[(3 * (indices[i] as usize) + c) as usize]);
            }
            rgba.push(0xFF);
        } else {
            for _ in 0..4 {
                rgba.push(0x00);
            }
        }
    }
    rgba
}

// TODO: handle this unwrap? i64 can handle ~200,000 years in microseconds
#[inline]
pub fn duration_to_f32(d: Duration) -> f32 {
    d.num_microseconds().unwrap() as f32 / 1_000_000.0
}

#[inline]
pub fn duration_from_f32(f: f32) -> Duration {
    Duration::microseconds((f * 1_000_000.0) as i64)
}

#[inline]
pub fn deg_vector_to_f32_vector(av: Vector3<Deg<f32>>) -> Vector3<f32> {
    Vector3::new(av[0].0, av[1].0, av[2].0)
}

#[inline]
pub fn deg_vector_from_f32_vector(v: Vector3<f32>) -> Vector3<Deg<f32>> {
    Vector3::new(Deg(v[0]), Deg(v[1]), Deg(v[2]))
}
