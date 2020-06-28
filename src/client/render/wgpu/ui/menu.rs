use std::collections::HashMap;

use crate::{
    client::{
        menu::{Item, Menu, MenuBodyView, MenuState, NamedMenuItem},
        render::wgpu::{
            ui::{
                glyph::{GlyphRendererCommand, GLYPH_HEIGHT, GLYPH_WIDTH},
                layout::{Anchor, Layout, ScreenPosition, Size},
                quad::{QuadRendererCommand, QuadTexture},
            },
            GraphicsState,
        },
    },
    common::wad::QPic,
};

use chrono::Duration;

// original minimum Quake resolution
const MENU_WIDTH: i32 = 320;
const MENU_HEIGHT: i32 = 200;

const SLIDER_LEFT: u8 = 128;
const SLIDER_MIDDLE: u8 = 129;
const SLIDER_RIGHT: u8 = 130;
const SLIDER_HANDLE: u8 = 131;
const SLIDER_WIDTH: i32 = 10;

#[derive(Clone, Copy, Debug)]
enum Align {
    Left,
    Center,
}

impl Align {
    pub fn x_ofs(&self) -> i32 {
        match *self {
            Align::Left => -MENU_WIDTH / 2,
            Align::Center => 0,
        }
    }

    pub fn anchor(&self) -> Anchor {
        match *self {
            Align::Left => Anchor::TOP_LEFT,
            Align::Center => Anchor::TOP_CENTER,
        }
    }
}

pub struct MenuRenderer {
    textures: HashMap<String, QuadTexture>,
}

impl MenuRenderer {
    pub fn new<'state>(state: &GraphicsState<'state>, menu: &Menu) -> MenuRenderer {
        let mut tex_names = std::collections::HashSet::new();
        tex_names.insert("gfx/qplaque.lmp".to_string());
        tex_names.extend((1..=6).into_iter().map(|i| format!("gfx/menudot{}.lmp", i)));
        let mut menus = vec![menu];

        // walk menu and collect necessary textures
        while let Some(m) = menus.pop() {
            tex_names.insert(m.view().title_path().to_string());

            if let MenuBodyView::Predefined { ref path, .. } = m.view().body() {
                tex_names.insert(path.to_string());
            }

            for item in m.items() {
                if let Item::Submenu(ref sub) = item.item() {
                    menus.push(sub);
                }
            }
        }

        MenuRenderer {
            textures: tex_names
                .into_iter()
                .map(|name| {
                    (
                        name.clone(),
                        QuadTexture::from_qpic(
                            state,
                            &QPic::load(state.vfs().open(&name).unwrap()).unwrap(),
                        ),
                    )
                })
                .collect(),
        }
    }

    fn texture<'state, S>(&self, state: &GraphicsState<'state>, name: S) -> &QuadTexture
    where
        S: AsRef<str>,
    {
        debug!("Fetch texture {}", name.as_ref());
        let qpic = QPic::load(state.vfs().open(name.as_ref()).unwrap()).unwrap();
        self.textures.get(name.as_ref()).unwrap()
    }

    fn cmd_draw_quad<'state>(
        &self,
        texture: &'state QuadTexture,
        align: Align,
        x_ofs: i32,
        y_ofs: i32,
        scale: f32,
        quad_cmds: &mut Vec<QuadRendererCommand<'state>>,
    ) {
        quad_cmds.push(QuadRendererCommand {
            texture,
            layout: Layout {
                position: ScreenPosition::Relative {
                    anchor: Anchor::CENTER,
                    x_ofs: align.x_ofs() + x_ofs,
                    y_ofs: MENU_HEIGHT / 2 + y_ofs,
                },
                anchor: align.anchor(),
                size: Size::Scale { factor: scale },
            },
        });
    }

    fn cmd_draw_glyph(
        &self,
        glyph_id: u8,
        x_ofs: i32,
        y_ofs: i32,
        scale: f32,
        glyph_cmds: &mut Vec<GlyphRendererCommand>,
    ) {
        glyph_cmds.push(GlyphRendererCommand::Glyph {
            glyph_id,
            position: ScreenPosition::Relative {
                anchor: Anchor::CENTER,
                x_ofs: -MENU_WIDTH / 2 + x_ofs,
                y_ofs: -MENU_HEIGHT / 2 + y_ofs,
            },
            anchor: Anchor::TOP_LEFT,
            scale,
        });
    }

    fn cmd_draw_plaque<'state, 'a>(
        &'a self,
        state: &GraphicsState<'state>,
        scale: f32,
        quad_cmds: &mut Vec<QuadRendererCommand<'a>>,
    ) {
        let plaque = self.texture(&state, "gfx/qplaque.lmp");
        self.cmd_draw_quad(plaque, Align::Left, 16, 4, scale, quad_cmds);
    }

    fn cmd_draw_title<'state, 'a, S>(
        &'a self,
        state: &GraphicsState<'state>,
        name: S,
        scale: f32,
        quad_cmds: &mut Vec<QuadRendererCommand<'a>>,
    ) where
        S: AsRef<str>,
    {
        let title = self.texture(state, name.as_ref());
        self.cmd_draw_quad(title, Align::Center, 0, 4, scale, quad_cmds);
    }

    fn cmd_draw_body_predef<'state, 'a, S>(
        &'a self,
        state: &GraphicsState<'state>,
        name: S,
        cursor_pos: usize,
        time: Duration,
        scale: f32,
        quad_cmds: &mut Vec<QuadRendererCommand<'a>>,
    ) where
        S: AsRef<str>,
    {
        let predef = self.texture(state, name.as_ref());
        self.cmd_draw_quad(predef, Align::Left, 72, -32, scale, quad_cmds);
        let curs_frame = (time.num_milliseconds() / 100) % 6;
        let curs = self.texture(state, &format!("gfx/menudot{}.lmp", curs_frame + 1));
        self.cmd_draw_quad(
            curs,
            Align::Left,
            72 - curs.width() as i32,
            -32 - cursor_pos as i32 * 20,
            scale,
            quad_cmds,
        );
    }

    fn cmd_draw_item_name<S>(
        &self,
        x: i32,
        y: i32,
        name: S,
        scale: f32,
        glyph_cmds: &mut Vec<GlyphRendererCommand>,
    ) where
        S: AsRef<str>,
    {
        glyph_cmds.push(GlyphRendererCommand::Text {
            text: name.as_ref().to_string(),
            position: ScreenPosition::Relative {
                anchor: Anchor::CENTER,
                x_ofs: -MENU_WIDTH / 2 + x - GLYPH_WIDTH as i32,
                y_ofs: -MENU_HEIGHT / 2 + y,
            },
            anchor: Anchor::TOP_RIGHT,
            scale,
        });
    }

    fn cmd_draw_item_text<S>(
        &self,
        x: i32,
        y: i32,
        text: S,
        scale: f32,
        glyph_cmds: &mut Vec<GlyphRendererCommand>,
    ) where
        S: AsRef<str>,
    {
        glyph_cmds.push(GlyphRendererCommand::Text {
            text: text.as_ref().to_string(),
            position: ScreenPosition::Relative {
                anchor: Anchor::CENTER,
                x_ofs: -MENU_WIDTH / 2 + x + GLYPH_WIDTH as i32,
                y_ofs: -MENU_HEIGHT / 2 + y,
            },
            anchor: Anchor::TOP_LEFT,
            scale,
        });
    }

    fn cmd_draw_slider(
        &self,
        x: i32,
        y: i32,
        pos: f32,
        scale: f32,
        glyph_cmds: &mut Vec<GlyphRendererCommand>,
    ) {
        self.cmd_draw_glyph(SLIDER_LEFT, x, y, scale, glyph_cmds);
        for i in 0..SLIDER_WIDTH {
            self.cmd_draw_glyph(SLIDER_MIDDLE, x + 8 * (i + 1), y, scale, glyph_cmds);
        }
        self.cmd_draw_glyph(SLIDER_RIGHT, x + 8 * SLIDER_WIDTH, y, scale, glyph_cmds);
        let handle_x = x + ((8 * (SLIDER_WIDTH - 1)) as f32 * pos) as i32;
        self.cmd_draw_glyph(SLIDER_HANDLE, handle_x, y, scale, glyph_cmds);
    }

    fn cmd_draw_body_dynamic<'state>(
        &self,
        state: &GraphicsState<'state>,
        items: &[NamedMenuItem],
        cursor_pos: usize,
        time: Duration,
        scale: f32,
        glyph_cmds: &mut Vec<GlyphRendererCommand>,
    ) {
        for (item_id, item) in items.iter().enumerate() {
            let y = MENU_HEIGHT - 32 - (GLYPH_HEIGHT * item_id) as i32;
            let x = 16 + 24 * GLYPH_WIDTH as i32;
            self.cmd_draw_item_name(x, y, item.name(), scale, glyph_cmds);

            match item.item() {
                Item::Toggle(toggle) => self.cmd_draw_item_text(
                    x,
                    y,
                    if toggle.get() { "yes" } else { "no" },
                    scale,
                    glyph_cmds,
                ),
                Item::Enum(e) => {
                    self.cmd_draw_item_text(x, y, e.selected_name(), scale, glyph_cmds)
                }
                Item::Slider(slider) => {
                    self.cmd_draw_slider(x, y, slider.position(), scale, glyph_cmds)
                }
                Item::TextField(_) => (),
                _ => (),
            }
        }

        if time.num_milliseconds() / 250 % 2 == 0 {
            self.cmd_draw_glyph(
                141,
                200,
                MENU_HEIGHT - 32 - 8 * cursor_pos as i32,
                scale,
                glyph_cmds,
            );
        }
    }

    pub fn generate_commands<'state, 'a>(
        &'a self,
        state: &GraphicsState<'state>,
        menu: &Menu,
        time: Duration,
        quad_cmds: &mut Vec<QuadRendererCommand<'a>>,
        glyph_cmds: &mut Vec<GlyphRendererCommand>,
    ) {
        let active_menu = menu.active_submenu().unwrap();
        let view = active_menu.view();

        // TODO: use cvar
        let scale = 2.0;

        if view.draw_plaque() {
            self.cmd_draw_plaque(state, scale, quad_cmds);
        }

        self.cmd_draw_title(state, view.title_path(), scale, quad_cmds);

        let cursor_pos = match active_menu.state() {
            MenuState::Active { index } => index,
            _ => unreachable!(),
        };

        match *view.body() {
            MenuBodyView::Predefined { ref path } => {
                self.cmd_draw_body_predef(state, path, cursor_pos, time, scale, quad_cmds);
            }
            MenuBodyView::Dynamic => {
                self.cmd_draw_body_dynamic(
                    state,
                    &active_menu.items(),
                    cursor_pos,
                    time,
                    scale,
                    glyph_cmds,
                );
            }
        }
    }
}
