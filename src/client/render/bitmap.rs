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

use client::render::ColorFormat;
use client::render::Palette;
use common::wad::QPic;

use failure::Error;
use gfx;
use gfx::Factory;
use gfx::format::R8_G8_B8_A8;
use gfx::handle::ShaderResourceView;
use gfx::handle::Texture;
use gfx_device_gl::Resources;

const RED_CHANNEL: usize = 0;
const GREEN_CHANNEL: usize = 1;
const BLUE_CHANNEL: usize = 2;
const ALPHA_CHANNEL: usize = 3;

#[derive(Clone, Debug)]
pub struct Bitmap {
    width: u32,
    height: u32,
    rgba: Box<[u8]>
}

impl Bitmap {
    pub fn new(width: u32, height: u32, rgba: Box<[u8]>) -> Result<Bitmap, Error> {
        ensure!(4 * (width * height) as usize == rgba.len(), "Invalid dimensions for given color data");

        Ok(Bitmap {
            width,
            height,
            rgba,
        })
    }

    pub fn from_qpic(qpic: &QPic, palette: &Palette) -> Result<Bitmap, Error> {
        let rgba = palette.indexed_to_rgba(qpic.indices());

        Bitmap::new(qpic.width(), qpic.height(), rgba.into_boxed_slice())
    }

    pub fn transparent(width: u32, height: u32) -> Result<Bitmap, Error> {
        Bitmap::new(width, height, vec![0; 4 * (width * height) as usize].into_boxed_slice())
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    fn xy_to_offset(&self, x: i32, y: i32) -> Option<usize> {
        if x >= 0 && x < self.width as i32 && y >= 0 && y < self.height as i32 {
            Some(4 * (y * self.width as i32 + x) as usize)
        } else {
            None
        }
    }

    pub fn pixel(&self, x: i32, y: i32) -> Option<&[u8]> {
        self.xy_to_offset(x, y).map(|ofs| &self.rgba[ofs..ofs + 4])
    }

    pub fn pixel_mut(&mut self, x: i32, y: i32) -> Option<&mut [u8]> {
        match self.xy_to_offset(x, y) {
            Some(ofs) => Some(&mut self.rgba[ofs..ofs + 4]),
            None => None,
        }
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    pub fn blit(&mut self, src: &Bitmap, x: i32, y: i32) {
        // TODO this is horribly unoptimized, calculate the intersection instead of testing every
        // pixel
        for src_y in 0..src.height() {
            for src_x in 0..src.width() {
                if let Some(pix) = self.pixel_mut(
                    x + src_x as i32,
                    y + src_y as i32
                ) {
                    let src_pix = src.pixel(src_x as i32, src_y as i32).unwrap();
                    match src_pix[ALPHA_CHANNEL] {
                        0xFF => pix.copy_from_slice(src_pix),
                        0x00 => (),

                        // rudimentary blending
                        alpha => {
                            let a1 = pix[ALPHA_CHANNEL] as f32;
                            let a2 = alpha as f32;
                            let result_alpha = a1 + a2;
                            let f1 = a1 / result_alpha;
                            let f2 = a2 / result_alpha;

                            let result = [
                                (f1 * pix[RED_CHANNEL] as f32 + f2 * src_pix[RED_CHANNEL] as f32) as u8,
                                (f1 * pix[GREEN_CHANNEL] as f32 + f2 * src_pix[GREEN_CHANNEL] as f32) as u8,
                                (f1 * pix[BLUE_CHANNEL] as f32 + f2 * src_pix[BLUE_CHANNEL] as f32) as u8,
                                if result_alpha > 255.0 { 255 } else { result_alpha as u8 },
                            ];

                            pix.copy_from_slice(&result);
                        }
                    }
                }
            }
        }
    }

    pub fn create_texture<F>(
        &self,
        factory: &mut F
    ) -> Result<(Texture<Resources, R8_G8_B8_A8>, ShaderResourceView<Resources, [f32; 4]>), Error>
    where
        F: Factory<Resources>
    {
        let result = factory.create_texture_immutable_u8::<ColorFormat>(
            gfx::texture::Kind::D2(
                self.width as u16,
                self.height as u16,
                gfx::texture::AaMode::Single
            ),
            gfx::texture::Mipmap::Allocated,
            &[&self.rgba],
        )?;

        Ok(result)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const RED: [u8; 4] = [0xFF, 0x00, 0x00, 0xFF];
    const GREEN: [u8; 4] = [0x00, 0xFF, 0x00, 0xFF];
    const BLUE: [u8; 4] = [0x00, 0x00, 0xFF, 0xFF];

    #[test]
    fn test_blit() {
        let mut b1_vec = Vec::new();
        for _ in 0..9 { b1_vec.extend_from_slice(&RED); }
        let mut b1 = Bitmap::new(3, 3, b1_vec.into_boxed_slice()).unwrap();

        let mut b2_vec = Vec::new();
        for _ in 0..9 { b2_vec.extend_from_slice(&BLUE); }
        let b2 = Bitmap::new(3, 3, b2_vec.into_boxed_slice()).unwrap();

        b1.blit(&b2, 1, 1);

        assert_eq!(b1.pixel(0, 0).unwrap(), &RED);
        assert_eq!(b1.pixel(1, 0).unwrap(), &RED);
        assert_eq!(b1.pixel(2, 0).unwrap(), &RED);
        assert_eq!(b1.pixel(0, 1).unwrap(), &RED);
        assert_eq!(b1.pixel(1, 1).unwrap(), &BLUE);
        assert_eq!(b1.pixel(2, 1).unwrap(), &BLUE);
        assert_eq!(b1.pixel(0, 2).unwrap(), &RED);
        assert_eq!(b1.pixel(1, 2).unwrap(), &BLUE);
        assert_eq!(b1.pixel(2, 2).unwrap(), &BLUE);
    }
}
