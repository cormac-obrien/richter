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

use std::cell::RefCell;
use std::rc::Rc;

use client::render::pipeline2d;
use client::render::glyph::{GLYPH_HEIGHT, GlyphRenderer, GLYPH_WIDTH};
use common::console::Console;

use failure::Error;
use gfx::{CommandBuffer, Encoder};
use gfx::pso::{PipelineData, PipelineState};
use gfx_device_gl::Resources;

const PAD_LEFT: i32 = GLYPH_WIDTH as i32;

pub struct ConsoleRenderer {
    console: Rc<RefCell<Console>>,
    glyph_renderer: Rc<GlyphRenderer>,
}

impl ConsoleRenderer {
    pub fn new(
        console: Rc<RefCell<Console>>,
        glyph_renderer: Rc<GlyphRenderer>,
    ) -> Result<ConsoleRenderer, Error> {
        Ok(ConsoleRenderer {
            console,
            glyph_renderer,
        })
    }

    pub fn render<C>(
        &self,
        encoder: &mut Encoder<Resources, C>,
        pso: &PipelineState<Resources, <pipeline2d::Data<Resources> as PipelineData<Resources>>::Meta>,
        user_data: &mut pipeline2d::Data<Resources>,
        display_width: u32,
        display_height: u32,
        proportion: f32,
        alpha: f32,
    ) -> Result<(), Error>
    where
        C: CommandBuffer<Resources>
    {
        ensure!(proportion >= 0.0 && proportion <= 1.0, "proportion must be between 0 and 1");
        ensure!(alpha >= 0.0 && alpha <= 1.0, "alpha must be between 0 and 1");

        // determine how far down the screen we should start drawing
        let y_min = ((1.0 - proportion) * display_height as f32) as i32;

        // draw version string
        // TODO: get this dynamically
        let version_string = String::from("Richter 0.1.0");
        self.glyph_renderer.render_string(
            encoder,
            pso,
            user_data,
            &version_string,
            display_width,
            display_height,
            display_width as i32 - (version_string.len() * GLYPH_WIDTH) as i32,
            y_min,
        )?;

        // TODO: draw input line

        // draw output
        for (line_id, line) in self.console.borrow().output_lines().enumerate() {
            for (chr_id, chr) in line.iter().enumerate() {
                let mut c = *chr;

                if c as u32 > ::std::u8::MAX as u32 {
                    warn!("char \"{}\" (U+{:4}) cannot be displayed in the console", c, c as u32);
                    continue;
                }

                self.glyph_renderer.render_glyph(
                    encoder,
                    pso,
                    user_data,
                    c as u8,
                    display_width,
                    display_height,
                    GLYPH_WIDTH as i32 * chr_id as i32,
                    y_min + GLYPH_HEIGHT as i32 * line_id as i32,
                )?;
            }
        }

        Ok(())
    }
}
