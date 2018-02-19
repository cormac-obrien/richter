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

use std::rc::Rc;

use common::bsp::BspData;
use common::pak::Pak;

use gfx;
use gfx::format::Srgba8 as ColorFormat;
use gfx::format::DepthStencil as DepthFormat;

const PALETTE_SIZE: usize = 768;

gfx_defines! {
    vertex Vertex {
        pos: [f32; 3] = "a_Pos",
        texcoord: [f32; 2] = "a_Texcoord",
    }

    constant Locals {
        transform: [[f32; 4]; 4] = "u_Transform",
    }

    pipeline pipe {
        vertex_buffer: gfx::VertexBuffer<Vertex> = (),
        transform: gfx::Global<[[f32; 4]; 4]> = "u_Transform",
        sampler: gfx::TextureSampler<[f32; 4]> = "u_Texture",
        out_color: gfx::RenderTarget<ColorFormat> = "Target0",
        out_depth: gfx::DepthTarget<DepthFormat> = gfx::preset::depth::LESS_EQUAL_WRITE,
    }
}

pub struct Palette {
    rgb: [[u8; 3]; 256],
}

impl Palette {
    pub fn load<S>(pak: &Pak, path: S) -> Palette
    where
        S: AsRef<str>,
    {
        let data = pak.open(path).unwrap();
        assert_eq!(data.len(), PALETTE_SIZE);

        let mut rgb = [[0u8; 3]; 256];

        for color in 0..256 {
            for component in 0..3 {
                rgb[color][component] = data[color * 3 + component];
            }
        }

        Palette { rgb }
    }

    // TODO: this will not render console characters correctly, as they use index 0 (black) to
    // indicate transparency.
    pub fn indexed_to_rgba(&self, indices: &[u8]) -> Vec<u8> {
        let mut rgba = Vec::with_capacity(indices.len() * 4);

        for index in indices {
            match *index {
                0xFF => for i in 0..4 {
                    rgba.push(0);
                },

                _ => {
                    for component in 0..3 {
                        rgba.push(self.rgb[*index as usize][component]);
                    }
                    rgba.push(0xFF);
                }
            }
        }

        rgba
    }
}
