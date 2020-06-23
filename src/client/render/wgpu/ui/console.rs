use crate::{
    client::render::wgpu::{
        ui::{
            glyph::{GlyphRendererCommand, GLYPH_HEIGHT, GLYPH_WIDTH},
            layout::{Anchor, AnchorCoord, ScreenPosition},
            quad::{QuadRendererCommand, QuadTexture},
        },
        GraphicsState,
    },
    common::{console::Console, wad::QPic},
};

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

    pub fn generate_commands<'a, 'b>(
        &'b self,
        state: &GraphicsState<'a>,
        console: &Console,
        quad_cmds: &mut Vec<QuadRendererCommand<'b>>,
        glyph_cmds: &mut Vec<GlyphRendererCommand>,
    ) {
        // TODO: take screen proportion as a parameter or cvar
        let proportion = 0.5;
        let console_anchor = Anchor {
            x: AnchorCoord::Zero,
            y: AnchorCoord::Proportion(1.0 - proportion),
        };

        // draw console background
        quad_cmds.push(QuadRendererCommand {
            texture: &self.conback,
            position: ScreenPosition::Absolute(console_anchor),
            anchor: Anchor::BOTTOM_LEFT,
        });

        // draw version string
        let version_string = format!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        glyph_cmds.push(GlyphRendererCommand::Text {
            text: version_string,
            position: ScreenPosition::Absolute(console_anchor),
            anchor: Anchor::BOTTOM_RIGHT,
        });

        // draw input line
        glyph_cmds.push(GlyphRendererCommand::Glyph {
            glyph_id: ']' as u8,
            position: ScreenPosition::Relative {
                anchor: console_anchor,
                x_ofs: PAD_LEFT,
                y_ofs: 0,
            },
            anchor: Anchor::BOTTOM_LEFT,
        });
        glyph_cmds.push(GlyphRendererCommand::Text {
            text: console.get_string(),
            position: ScreenPosition::Relative {
                anchor: console_anchor,
                x_ofs: PAD_LEFT + GLYPH_WIDTH as i32,
                y_ofs: 0,
            },
            anchor: Anchor::BOTTOM_LEFT,
        });

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
                    y_ofs: (line_id * GLYPH_HEIGHT) as i32,
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
                });
            }
        }
    }
}
