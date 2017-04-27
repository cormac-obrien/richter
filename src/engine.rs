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

extern crate glium;

use std::fs::File;
use std::io::Read;

use glium::Texture2d;
use glium::backend::glutin_backend::GlutinFacade as Window;
use glium::texture::RawImage2d;

lazy_static! {
    static ref PALETTE: [u8; 768] = {
        let mut _palette = [0; 768];
        let mut f = File::open("pak0/gfx/palette.lmp").unwrap();
        match f.read(&mut _palette) {
            Err(why) => panic!("{}", why),
            Ok(768) => _palette,
            _ => panic!("Bad read on pak0/gfx/palette.lmp"),
        }
    };
}

pub fn tex_from_indexed(window: &Window, indices: &[u8], width: u32, height: u32) -> Texture2d {
    if indices.len() != (width * height) as usize {
        panic!("Bad index list length: {}", indices.len());
    }

    let rgba: Vec<u8> = {
        let mut _rgba: Vec<u8> = Vec::with_capacity(4 * indices.len());
        for i in 0..indices.len() {
            if indices[i] != 0xff {
                for c in 0..3 {
                    _rgba.push(PALETTE[(3 * (indices[i] as usize) + c) as usize]);
                }
                _rgba.push(0xff);
            } else {
                for _ in 0..4 {
                    _rgba.push(0);
                }
            }
        }
        _rgba
    };

    let raw_image = RawImage2d::from_raw_rgba(rgba, (width, height));

    Texture2d::new(window, raw_image).unwrap()
}
