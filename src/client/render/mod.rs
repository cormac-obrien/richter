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

use common::pak::Pak;

use cgmath::Deg;
use cgmath::Euler;
use cgmath::Matrix4;
use cgmath::Vector3;
use chrono::Duration;
use gfx;

pub use gfx::format::Srgba8 as ColorFormat;
pub use gfx::format::DepthStencil as DepthFormat;

use self::bsp::BspRenderer;

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

pub struct Camera {
    origin: Vector3<f32>,
    angles: Euler<Deg<f32>>,
    projection: Matrix4<f32>,

    transform: Matrix4<f32>,
}

impl Camera {
    pub fn new(
        origin: Vector3<f32>,
        angles: Euler<Deg<f32>>,
        projection: Matrix4<f32>,
    ) -> Camera {
        Camera {
            origin,
            angles,
            projection,
            // negate the camera origin and angles
            // TODO: the OpenGL coordinate conversion is hardcoded here!
            transform: projection * Matrix4::from(Euler::new(-angles.x, -angles.y, -angles.z))
                * Matrix4::from_translation(-Vector3::new(-origin.y, origin.z, -origin.x)),
        }
    }

    pub fn get_origin(&self) -> Vector3<f32> {
        self.origin
    }

    pub fn get_transform(&self) -> Matrix4<f32> {
        self.transform
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
