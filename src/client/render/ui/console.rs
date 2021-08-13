use crate::{
    client::render::{
        ui::{
            glyph::{GlyphRendererCommand, GLYPH_HEIGHT, GLYPH_WIDTH},
            layout::{Anchor, AnchorCoord, Layout, ScreenPosition, Size},
            quad::{QuadRendererCommand, QuadTexture},
        },
        GraphicsState,
    },
    common::{console::Console, engine, wad::QPic},
};

use chrono::Duration;

const PAD_LEFT: i32 = GLYPH_WIDTH as i32;

pub struct ConsoleRenderer {
    conback: QuadTexture,
}

impl ConsoleRenderer {
    pub fn new(state: &GraphicsState) -> ConsoleRenderer {
        let conback = QuadTexture::from_qpic(
            state,
            &QPic::load(state.vfs().open("gfx/conback.lmp").unwrap()).unwrap(),
        );

        ConsoleRenderer { conback }
    }

    pub fn generate_commands<'a>(
        &'a self,
        console: &Console,
        time: Duration,
        quad_cmds: &mut Vec<QuadRendererCommand<'a>>,
        glyph_cmds: &mut Vec<GlyphRendererCommand>,
        proportion: f32,
    ) {
        // TODO: take scale as cvar
        let scale = 2.0;
        let console_anchor = Anchor {
            x: AnchorCoord::Zero,
            y: AnchorCoord::Proportion(1.0 - proportion),
        };

        // draw console background
        quad_cmds.push(QuadRendererCommand {
            texture: &self.conback,
            layout: Layout {
                position: ScreenPosition::Absolute(console_anchor),
                anchor: Anchor::BOTTOM_LEFT,
                size: Size::DisplayScale { ratio: 1.0 },
            },
        });

        // draw version string
        let version_string = format!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        glyph_cmds.push(GlyphRendererCommand::Text {
            text: version_string,
            position: ScreenPosition::Absolute(console_anchor),
            anchor: Anchor::BOTTOM_RIGHT,
            scale,
        });

        // draw input line
        glyph_cmds.push(GlyphRendererCommand::Glyph {
            glyph_id: b']',
            position: ScreenPosition::Relative {
                anchor: console_anchor,
                x_ofs: PAD_LEFT,
                y_ofs: 0,
            },
            anchor: Anchor::BOTTOM_LEFT,
            scale,
        });
        let input_text = console.get_string();
        glyph_cmds.push(GlyphRendererCommand::Text {
            text: input_text,
            position: ScreenPosition::Relative {
                anchor: console_anchor,
                x_ofs: PAD_LEFT + GLYPH_WIDTH as i32,
                y_ofs: 0,
            },
            anchor: Anchor::BOTTOM_LEFT,
            scale,
        });
        // blink cursor in half-second intervals
        if engine::duration_to_f32(time).fract() > 0.5 {
            glyph_cmds.push(GlyphRendererCommand::Glyph {
                glyph_id: 11,
                position: ScreenPosition::Relative {
                    anchor: console_anchor,
                    x_ofs: PAD_LEFT + (GLYPH_WIDTH * (console.cursor() + 1)) as i32,
                    y_ofs: 0,
                },
                anchor: Anchor::BOTTOM_LEFT,
                scale,
            });
        }

        // draw previous output
        for (line_id, line) in console.output().lines().enumerate() {
            // TODO: implement scrolling
            if line_id > 100 {
                break;
            }

            for (chr_id, chr) in line.iter().enumerate() {
                let position = ScreenPosition::Relative {
                    anchor: console_anchor,
                    x_ofs: PAD_LEFT + (1 + chr_id * GLYPH_WIDTH) as i32,
                    y_ofs: ((line_id + 1) * GLYPH_HEIGHT) as i32,
                };

                let c = if *chr as u32 > std::u8::MAX as u32 {
                    warn!(
                        "char \"{}\" (U+{:4}) cannot be displayed in the console",
                        *chr, *chr as u32
                    );
                    '?'
                } else {
                    *chr
                };

                glyph_cmds.push(GlyphRendererCommand::Glyph {
                    glyph_id: c as u8,
                    position,
                    anchor: Anchor::BOTTOM_LEFT,
                    scale,
                });
            }
        }
    }
}
