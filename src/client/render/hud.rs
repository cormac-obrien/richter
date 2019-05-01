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

use std::cell::RefCell;
use std::rc::Rc;

use client::render::bitmap::BitmapTexture;
use client::render::glyph::GlyphRendererCommand;
use client::render::{self, GraphicsPackage, PipelineData2d, Vertex2d};
use client::Client;
use common::net::{ClientStat, ItemFlags};

use chrono::Duration;
use flame;
use gfx::handle::Buffer;
use gfx::{CommandBuffer, Encoder};
use gfx_device_gl::Resources;

use failure::Error;

enum WeaponSlots {
    Shotgun = 0,
    SuperShotgun = 1,
    Nailgun = 2,
    SuperNailgun = 3,
    RocketLauncher = 4,
    GrenadeLauncher = 5,
    LightningGun = 6,
}

enum AmmoSlots {
    Shells = 0,
    Nails = 1,
    Rockets = 2,
    Cells = 3,
}

pub struct HudRenderer {
    gfx_pkg: Rc<RefCell<GraphicsPackage>>,

    digits: Box<[BitmapTexture]>,
    minus: BitmapTexture,
    alt_digits: Box<[BitmapTexture]>,
    alt_minus: BitmapTexture,
    colon: BitmapTexture,
    slash: BitmapTexture,

    weapons: Box<[Box<[BitmapTexture]>]>,
    ammo: Box<[BitmapTexture]>,
    sigils: Box<[BitmapTexture]>,
    armor: Box<[BitmapTexture]>,
    items: Box<[BitmapTexture]>,
    faces: Box<[BitmapTexture]>,
    pain_faces: Box<[BitmapTexture]>,

    face_invis: BitmapTexture,
    face_invuln: BitmapTexture,
    face_invis_invuln: BitmapTexture,
    face_quad: BitmapTexture,
    sbar: BitmapTexture,
    ibar: BitmapTexture,
    scorebar: BitmapTexture,

    vertex_buffer: Buffer<Resources, Vertex2d>,
}

impl HudRenderer {
    pub fn new(gfx_pkg: Rc<RefCell<GraphicsPackage>>) -> Result<HudRenderer, Error> {
        use gfx::traits::FactoryExt;
        let vertex_buffer = {
            let pkg_mut = gfx_pkg.borrow_mut();
            let mut factory_mut = pkg_mut.factory_mut();
            factory_mut.create_vertex_buffer(&render::QUAD_VERTICES)
        };

        let mut digits = Vec::new();
        let mut alt_digits = Vec::new();

        // just to make the following code a bit less painful
        let qpic_to_bitmap = |name: &str| -> Result<BitmapTexture, Error> {
            let pkg = gfx_pkg.clone();
            let qpic = pkg.borrow().gfx_wad().open_qpic(name)?;
            let pkg_mut = pkg.borrow_mut();
            use std::ops::DerefMut;
            let tex = BitmapTexture::from_qpic(
                pkg_mut.factory_mut().deref_mut(),
                &qpic,
                pkg_mut.palette(),
            )?;
            Ok(tex)
        };

        for i in 0..10 {
            digits.push(qpic_to_bitmap(&format!("NUM_{}", i))?);
            alt_digits.push(qpic_to_bitmap(&format!("ANUM_{}", i))?);
        }

        let minus = qpic_to_bitmap("NUM_MINUS")?;
        let alt_minus = qpic_to_bitmap("ANUM_MINUS")?;
        let colon = qpic_to_bitmap("NUM_COLON")?;
        let slash = qpic_to_bitmap("NUM_SLASH")?;

        let weapon_names = vec![
            "SHOTGUN", "SSHOTGUN", "NAILGUN", "SNAILGUN", "RLAUNCH", "SRLAUNCH", "LIGHTNG",
        ];

        let weapon_prefixes = vec!["", "2", "A1", "A2", "A3", "A4", "A5"];

        let mut weapons = Vec::new();

        for w_name in &weapon_names {
            let mut weapon_frames = Vec::new();

            for w_prefix in &weapon_prefixes {
                weapon_frames.push(qpic_to_bitmap(&format!("INV{}_{}", w_prefix, w_name))?);
            }

            weapons.push(weapon_frames.into_boxed_slice());
        }

        let ammo_names = vec!["SHELLS", "NAILS", "ROCKET", "CELLS"];
        let mut ammo = Vec::new();
        for ammo_name in &ammo_names {
            ammo.push(qpic_to_bitmap(&format!("SB_{}", ammo_name))?);
        }

        let mut armor = Vec::new();
        for i in 1..4 {
            armor.push(qpic_to_bitmap(&format!("SB_ARMOR{}", i))?);
        }

        let item_names = vec!["KEY1", "KEY2", "INVIS", "INVULN", "SUIT", "QUAD"];
        let mut items = Vec::new();
        for item_name in &item_names {
            items.push(qpic_to_bitmap(&format!("SB_{}", item_name))?);
        }

        let mut sigils = Vec::new();
        for i in 1..5 {
            sigils.push(qpic_to_bitmap(&format!("SB_SIGIL{}", i))?);
        }

        let mut faces = Vec::new();
        let mut pain_faces = Vec::new();
        for i in 1..6 {
            faces.push(qpic_to_bitmap(&format!("FACE{}", i))?);
            pain_faces.push(qpic_to_bitmap(&format!("FACE_P{}", i))?);
        }

        let face_invis = qpic_to_bitmap("FACE_INVIS")?;
        let face_invuln = qpic_to_bitmap("FACE_INVUL2")?;
        let face_invis_invuln = qpic_to_bitmap("FACE_INV2")?;
        let face_quad = qpic_to_bitmap("FACE_QUAD")?;

        let sbar = qpic_to_bitmap("SBAR")?;
        let ibar = qpic_to_bitmap("IBAR")?;
        let scorebar = qpic_to_bitmap("SCOREBAR")?;

        // TODO: use a cvar to determine HUD scaling (for now, do 2:1)

        Ok(HudRenderer {
            gfx_pkg: gfx_pkg.clone(),

            digits: digits.into_boxed_slice(),
            minus,
            alt_digits: alt_digits.into_boxed_slice(),
            alt_minus,
            colon,
            slash,

            weapons: weapons.into_boxed_slice(),
            ammo: ammo.into_boxed_slice(),
            sigils: sigils.into_boxed_slice(),
            armor: armor.into_boxed_slice(),
            items: items.into_boxed_slice(),
            faces: faces.into_boxed_slice(),
            pain_faces: pain_faces.into_boxed_slice(),

            face_invis,
            face_invuln,
            face_invis_invuln,
            face_quad,
            sbar,
            ibar,
            scorebar,

            vertex_buffer,
        })
    }

    pub fn render_bitmap<C>(
        &self,
        bitmap: &BitmapTexture,
        encoder: &mut Encoder<Resources, C>,
        user_data: &mut PipelineData2d,
        display_width: u32,
        display_height: u32,
        position_x: i32,
        position_y: i32,
    ) where
        C: CommandBuffer<Resources>,
    {
        user_data.vertex_buffer = self.vertex_buffer.clone();
        user_data.transform = bitmap
            .transform(display_width, display_height, position_x, position_y)
            .into();
        user_data.sampler.0 = bitmap.view();
        encoder.draw(
            &render::QUAD_SLICE,
            &self.gfx_pkg.borrow().pipeline_2d(),
            user_data,
        );
    }

    pub fn render_number<C>(
        &self,
        number: i32,
        max_digits: usize,
        alt_color: bool,
        encoder: &mut Encoder<Resources, C>,
        user_data: &mut PipelineData2d,
        display_width: u32,
        display_height: u32,
        position_x: i32,
        position_y: i32,
    ) where
        C: CommandBuffer<Resources>,
    {
        let number_str = format!("{}", number);
        let number_chars: Vec<char> = number_str.chars().collect();

        let mut skip = 0;
        let mut ofs = 0;
        if number_chars.len() > max_digits {
            skip = number_chars.len() - max_digits;
        } else if max_digits > number_chars.len() {
            ofs = (max_digits - number_chars.len()) as i32 * 24;
        }

        for (chr_id, chr) in number_chars.into_iter().skip(skip).enumerate() {
            self.render_bitmap(
                match chr {
                    '-' => {
                        if alt_color {
                            &self.alt_minus
                        } else {
                            &self.minus
                        }
                    }
                    '0'..='9' => {
                        let index = chr as usize - '0' as usize;
                        if alt_color {
                            &self.alt_digits[index]
                        } else {
                            &self.digits[index]
                        }
                    }
                    _ => unreachable!(),
                },
                encoder,
                user_data,
                display_width,
                display_height,
                position_x + ofs + 24 * chr_id as i32,
                position_y,
            );
        }
    }

    pub fn render<C>(
        &mut self,
        encoder: &mut Encoder<Resources, C>,
        client: &Client,
        display_width: u32,
        display_height: u32,
    ) -> Result<(), Error>
    where
        C: CommandBuffer<Resources>,
    {
        let mut user_data = self.gfx_pkg.borrow().gen_user_data_2d();

        // TODO: scale using a cvar (Quakespasm uses scr_{con,crosshair,menu,sbar}scale)
        let display_width = display_width / 2;
        let display_height = display_height / 2;

        let _guard = flame::start_guard("HudRenderer::render");

        let sbar_x = (display_width - self.sbar.width()) as i32 / 2;
        let sbar_y = 0i32;

        let ibar_x = sbar_x;
        let ibar_y = sbar_y + self.sbar.height() as i32;

        self.render_bitmap(
            &self.sbar,
            encoder,
            &mut user_data,
            display_width,
            display_height,
            sbar_x,
            sbar_y,
        );
        self.render_bitmap(
            &self.ibar,
            encoder,
            &mut user_data,
            display_width,
            display_height,
            ibar_x,
            ibar_y,
        );

        // weapons
        for i in 0..8 {
            if client
                .items()
                .contains(ItemFlags::from_bits(ItemFlags::SHOTGUN.bits() << i).unwrap())
            {
                let get_time = client.item_get_time()[i];
                let delta = client.time() - get_time;
                let flash_on = if delta >= Duration::milliseconds(100) {
                    if client.active_weapon() as u32 == ItemFlags::SHOTGUN.bits() << i {
                        1
                    } else {
                        0
                    }
                } else {
                    (delta.num_milliseconds() * 100) as usize % 5 + 2
                };

                self.render_bitmap(
                    &self.weapons[i][flash_on],
                    encoder,
                    &mut user_data,
                    display_width,
                    display_height,
                    sbar_x + 24 * i as i32,
                    sbar_y + self.sbar.height() as i32,
                );
            }
        }

        // ammo
        for i in 0..4 {
            let ammo_str = format!("{: >3}", client.stats()[ClientStat::Shells as usize + i]);
            for (chr_id, chr) in ammo_str.chars().enumerate() {
                if chr != ' ' {
                    self.gfx_pkg.borrow().glyph_renderer().render_command(
                        encoder,
                        self.gfx_pkg.borrow().pipeline_2d(),
                        &mut user_data,
                        display_width,
                        display_height,
                        GlyphRendererCommand::glyph(
                            18 + chr as u8 - '0' as u8,
                            ibar_x + (6 * i + chr_id) as i32 * 8 + 10,
                            ibar_y + 16,
                        ),
                    )?;
                }
            }
        }

        for i in 0..6 {
            if client
                .items()
                .contains(ItemFlags::from_bits(ItemFlags::KEY_1.bits() << i).unwrap())
            {
                let get_time = client.item_get_time()[17 + i];
                let _delta = client.time() - get_time;

                // TODO: add !hipnotic as a condition
                if i > 1 {
                    self.render_bitmap(
                        &self.items[i],
                        encoder,
                        &mut user_data,
                        display_width,
                        display_height,
                        sbar_x + 192 + 16 * i as i32,
                        sbar_y + self.sbar.height() as i32,
                    );
                }
            }
        }

        for i in 0..4 {
            if client
                .items()
                .contains(ItemFlags::from_bits(ItemFlags::SIGIL_1.bits() << i).unwrap())
            {
                let _get_time = client.item_get_time()[28 + i];
                self.render_bitmap(
                    &self.sigils[i],
                    encoder,
                    &mut user_data,
                    display_width,
                    display_height,
                    sbar_x + 288 + 8 * i as i32,
                    sbar_y + self.sbar.height() as i32,
                );
            }
        }

        // armor
        if client.items().contains(ItemFlags::INVULNERABILITY) {
            self.render_number(
                666,
                3,
                true,
                encoder,
                &mut user_data,
                display_width,
                display_height,
                sbar_x + 24,
                sbar_y,
            );
        // TODO: draw_disc
        } else {
            let armor = client.stats()[ClientStat::Armor as usize];
            self.render_number(
                armor,
                3,
                armor <= 25,
                encoder,
                &mut user_data,
                display_width,
                display_height,
                sbar_x + 24,
                sbar_y,
            );

            let armor_pic_id = if client.items().contains(ItemFlags::ARMOR_3) {
                Some(2)
            } else if client.items().contains(ItemFlags::ARMOR_2) {
                Some(1)
            } else if client.items().contains(ItemFlags::ARMOR_1) {
                Some(0)
            } else {
                None
            };

            if let Some(i) = armor_pic_id {
                self.render_bitmap(
                    &self.armor[i],
                    encoder,
                    &mut user_data,
                    display_width,
                    display_height,
                    sbar_x,
                    sbar_y,
                );
            }
        }

        // health
        let health = client.stats()[ClientStat::Health as usize];
        self.render_number(
            health,
            3,
            health <= 25,
            encoder,
            &mut user_data,
            display_width,
            display_height,
            sbar_x + 136,
            sbar_y,
        );

        let ammo_id = if client.items().contains(ItemFlags::SHELLS) {
            Some(0)
        } else if client.items().contains(ItemFlags::NAILS) {
            Some(1)
        } else if client.items().contains(ItemFlags::ROCKETS) {
            Some(2)
        } else if client.items().contains(ItemFlags::CELLS) {
            Some(3)
        } else {
            None
        };

        if let Some(i) = ammo_id {
            self.render_bitmap(
                &self.ammo[i],
                encoder,
                &mut user_data,
                display_width,
                display_height,
                sbar_x + 224,
                sbar_y,
            );
        }

        let ammo = client.stats()[ClientStat::Ammo as usize];
        self.render_number(
            ammo,
            3,
            ammo <= 10,
            encoder,
            &mut user_data,
            display_width,
            display_height,
            sbar_x + 248,
            sbar_y,
        );

        Ok(())
    }
}
