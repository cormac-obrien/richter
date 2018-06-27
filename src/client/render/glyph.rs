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

use client::render::{self, Palette, Vertex2d};
use client::render::pipeline2d;
use common::wad::QPic;

use cgmath::Matrix4;
use failure::Error;
use gfx::{CommandBuffer, Encoder, Factory, IndexBuffer, Slice};
use gfx::handle::{Buffer, ShaderResourceView, Texture};
use gfx::pso::{PipelineData, PipelineState};
use gfx_device_gl::Resources;

pub const GLYPH_WIDTH: usize = 8;
pub const GLYPH_HEIGHT: usize = 8;
const GLYPH_COLS: usize = 16;
const GLYPH_ROWS: usize = 16;
const GLYPH_COUNT: usize = GLYPH_ROWS * GLYPH_COLS;
const GLYPH_TEXTURE_WIDTH: usize = GLYPH_WIDTH * GLYPH_COLS;
const GLYPH_TEXTURE_HEIGHT: usize = GLYPH_HEIGHT * GLYPH_ROWS;
const GLYPH_WIDTH_TEXCOORD: f32 = 1.0 / GLYPH_COLS as f32;
const GLYPH_HEIGHT_TEXCOORD: f32 = 1.0 / GLYPH_ROWS as f32;

pub enum GlyphRendererCommand {
    Glyph {
        glyph_id: u8,
        x: i32,
        y: i32,
    },

    Text {
        text: String,
        x: i32,
        y: i32,
    }
}

impl GlyphRendererCommand {
    pub fn glyph(glyph_id: u8, x: i32, y: i32) -> GlyphRendererCommand {
        GlyphRendererCommand::Glyph { glyph_id, x, y }
    }

    pub fn text(text: String, x: i32, y: i32) -> GlyphRendererCommand {
        GlyphRendererCommand::Text { text, x, y }
    }
}

#[derive(Debug)]
pub struct GlyphRenderer {
    vertex_buffer: Buffer<Resources, Vertex2d>,
    view: ShaderResourceView<Resources, [f32; 4]>,
}

impl GlyphRenderer {
    pub fn new<F>(factory: &mut F, qpic: &QPic, palette: &Palette) -> Result<GlyphRenderer, Error>
    where
        F: Factory<Resources>
    {
        ensure!(qpic.width() as usize == GLYPH_COLS * GLYPH_WIDTH, "bad width for glyph atlas ({})", qpic.width());
        ensure!(qpic.height() as usize == GLYPH_ROWS * GLYPH_HEIGHT, "bad height for glyph atlas ({})", qpic.height());

        // conchars use index 0 (black) for transparency, so substitute index 0xFF
        let indices: Vec<u8> = qpic.indices()
            .iter()
            .map(|i| if *i == 0 { 0xFF } else { *i })
            .collect();

        let (rgba, _fullbright) = palette.translate(&indices);

        let (_handle, view) = render::create_texture(
            factory,
            GLYPH_TEXTURE_WIDTH as u32,
            GLYPH_TEXTURE_HEIGHT as u32,
            &rgba,
        )?;

        // create a quad for each glyph
        // TODO: these could be indexed to save space and maybe get slightly better cache coherence
        let mut vertices = Vec::new();
        for row in 0..GLYPH_ROWS {
            for col in 0..GLYPH_COLS {
                for vert in &render::QUAD_VERTICES {
                    vertices.push(Vertex2d {
                        pos: vert.pos,
                        texcoord: [
                            (col as f32 + vert.texcoord[0]) * GLYPH_WIDTH_TEXCOORD,
                            (row as f32 + vert.texcoord[1]) * GLYPH_HEIGHT_TEXCOORD,
                        ],
                    });
                }
            }
        }

        use gfx::traits::FactoryExt;
        let vertex_buffer = factory.create_vertex_buffer(&vertices);

        Ok(GlyphRenderer {
            vertex_buffer,
            view,
        })
    }

    fn slice_for_glyph(&self, glyph_id: u8) -> Slice<Resources> {
        Slice {
            start: 0,
            end: 6,
            base_vertex: 6 * glyph_id as u32,
            instances: None,
            buffer: IndexBuffer::Auto,
        }
    }

    pub fn render_command<C>(
        &self,
        encoder: &mut Encoder<Resources, C>,
        pso: &PipelineState<Resources, <pipeline2d::Data<Resources> as PipelineData<Resources>>::Meta>,
        user_data: &mut pipeline2d::Data<Resources>,
        display_width: u32,
        display_height: u32,
        command: GlyphRendererCommand,
    ) -> Result<(), Error>
    where
        C: CommandBuffer<Resources>,
    {
        user_data.vertex_buffer = self.vertex_buffer.clone();
        user_data.sampler.0 = self.view.clone();

        match command {
            GlyphRendererCommand::Glyph { glyph_id, x, y } => {
                user_data.transform = render::screen_space_vertex_transform(
                    display_width,
                    display_height,
                    GLYPH_WIDTH as u32,
                    GLYPH_HEIGHT as u32,
                    x,
                    y,
                ).into();
                let slice = self.slice_for_glyph(glyph_id);
                encoder.draw(&slice, pso, user_data);
            }

            GlyphRendererCommand::Text { text, x, y } => {
                for (chr_id, chr) in text.as_str().chars().enumerate() {
                    let abs_x = x + (GLYPH_WIDTH as i32 * chr_id as i32);

                    if abs_x >= display_width as i32 {
                        // this is off the edge of the screen, don't bother rendering
                        break;
                    }

                    user_data.transform = render::screen_space_vertex_transform(
                        display_width,
                        display_height,
                        GLYPH_WIDTH as u32,
                        GLYPH_HEIGHT as u32,
                        abs_x,
                        y,
                    ).into();

                    // TODO: check ASCII -> conchar mapping
                    let slice = self.slice_for_glyph(chr as u8);
                    encoder.draw(&slice, pso, user_data);
                }
            }
        }

        Ok(())
    }
}
