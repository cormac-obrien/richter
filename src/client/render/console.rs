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

use client::render::{self, Palette, Vertex2d};
use client::render::bitmap::BitmapTexture;
use client::render::glyph::{GLYPH_HEIGHT, GlyphRenderer, GlyphRendererCommand, GLYPH_WIDTH};
use client::render::pipeline2d;
use common::console::Console;
use common::pak::Pak;
use common::wad::QPic;

use failure::Error;
use gfx::{CommandBuffer, Encoder, Factory, Slice};
use gfx::handle::Buffer;
use gfx::pso::{PipelineData, PipelineState};
use gfx_device_gl::Resources;

const PAD_LEFT: i32 = GLYPH_WIDTH as i32;

pub struct ConsoleRenderer {
    console: Rc<RefCell<Console>>,
    glyph_renderer: Rc<GlyphRenderer>,
    vertex_buffer: Buffer<Resources, Vertex2d>,
    slice: Slice<Resources>,
    conback: BitmapTexture,
}

impl ConsoleRenderer {
    pub fn new<F>(
        pak: &Pak,
        factory: &mut F,
        vertex_buffer: Buffer<Resources, Vertex2d>,
        palette: &Palette,
        console: Rc<RefCell<Console>>,
        glyph_renderer: Rc<GlyphRenderer>,
    ) -> Result<ConsoleRenderer, Error>
    where F: Factory<Resources> {
        let slice = Slice::new_match_vertex_buffer(&vertex_buffer);
        let conback = BitmapTexture::from_qpic(factory, &QPic::load(pak.open("gfx/conback.lmp").unwrap())?, palette)?;
        Ok(ConsoleRenderer {
            console,
            glyph_renderer,
            vertex_buffer,
            slice,
            conback,
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

        // TODO: replace with cvar scr_conscale
        let display_width = display_width / 2;
        let display_height = display_height / 2;

        // determine how far down the screen we should start drawing
        let y_min = ((1.0 - proportion) * display_height as f32) as i32;

        // draw background
        user_data.vertex_buffer = self.vertex_buffer.clone();
        user_data.transform = render::screen_space_vertex_transform(
            display_width,
            display_height,
            display_width,
            display_height,
            0,
            y_min
        ).into();
        user_data.sampler.0 = self.conback.view();
        encoder.draw(&self.slice, pso, user_data);

        let mut commands = Vec::new();

        // draw version string
        // TODO: get this dynamically
        let version_string = String::from("Richter 0.1.0");
        commands.push(GlyphRendererCommand::text(
            version_string.to_owned(),
            display_width as i32 - (version_string.len() * GLYPH_WIDTH) as i32,
            y_min
        ));

        // draw input line
        commands.push(GlyphRendererCommand::glyph(']' as u8, PAD_LEFT as i32, y_min + GLYPH_HEIGHT as i32));
        commands.push(GlyphRendererCommand::text(
            self.console.borrow().get_string(),
            PAD_LEFT as i32 + GLYPH_WIDTH as i32,
            y_min + GLYPH_HEIGHT as i32,
        ));

        // draw output
        let console = self.console.borrow();
        for (line_id, line) in console.output_lines().enumerate() {
            // TODO: actually calculate the maximum extent of the console and stop rendering there
            // this will be needed for scrolling functionality
            if line_id > 100 {
                break;
            }

            for (chr_id, chr) in line.iter().enumerate() {
                let mut c = *chr;

                if c as u32 > ::std::u8::MAX as u32 {
                    warn!("char \"{}\" (U+{:4}) cannot be displayed in the console", c, c as u32);
                    continue;
                }

                commands.push(GlyphRendererCommand::glyph(
                    c as u8,
                    PAD_LEFT as i32 + GLYPH_WIDTH as i32 * chr_id as i32,
                    // line_id + 2 is the row above the input line
                    y_min + GLYPH_HEIGHT as i32 * (line_id + 2) as i32,
                ));
            }
        }

        for command in commands {
            self.glyph_renderer.render_command(
                encoder,
                pso,
                user_data,
                display_width,
                display_height,
                command,
            )?;
        }

        Ok(())
    }
}
