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

use std::mem;

use client::Client;
use client::render::Palette;
use client::render::Vertex2d;
use client::render::bitmap::BitmapTexture;
use client::render::pipeline2d;
use common::net::ClientStat;
use common::net::ItemFlags;
use common::net::MAX_ITEMS;
use common::wad::Wad;

use cgmath::Matrix4;
use cgmath::SquareMatrix;
use chrono::Duration;
use flame;
use gfx::CommandBuffer;
use gfx::Encoder;
use gfx::Factory;
use gfx::IndexBuffer;
use gfx::Slice;
use gfx::format::R8_G8_B8_A8;
use gfx::handle::Buffer;
use gfx::handle::ShaderResourceView;
use gfx::handle::Texture;
use gfx::pso::PipelineData;
use gfx::pso::PipelineState;
use gfx_device_gl::Resources;

use failure::Error;

// these have to be wound clockwise
// t-texcoords are currently inverted because bitmaps are defined with t=0 at the top
static FULLSCREEN_QUAD: [Vertex2d; 6] = [
    Vertex2d { pos: [-1.0, -1.0], texcoord: [0.0, 1.0] }, // bottom left
    Vertex2d { pos: [-1.0, 1.0], texcoord: [0.0, 0.0] }, // top left
    Vertex2d { pos: [1.0, 1.0], texcoord: [1.0, 0.0] }, // top right
    Vertex2d { pos: [-1.0, -1.0], texcoord: [0.0, 1.0] }, // bottom left
    Vertex2d { pos: [1.0, 1.0], texcoord: [1.0, 0.0] }, // top right
    Vertex2d { pos: [1.0, -1.0], texcoord: [1.0, 1.0] }, // bottom right
];

static FULLSCREEN_SLICE: Slice<Resources> = Slice {
    start: 0,
    end: 6,
    base_vertex: 0,
    instances: None,
    buffer: IndexBuffer::Auto,
};

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
    digits: Box<[BitmapTexture]>,
    minus: BitmapTexture,
    alt_digits: Box<[BitmapTexture]>,
    alt_minus: BitmapTexture,
    colon: BitmapTexture,
    slash: BitmapTexture,

    weapons: Box<[Box<[BitmapTexture]>]>,
    ammo: Box<[BitmapTexture]>,
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
    pub fn new<F>(
        gfx_wad: &Wad,
        palette: &Palette,
        factory: &mut F
    ) -> Result<HudRenderer, Error>
    where
        F: Factory<Resources>
    {
        use gfx::traits::FactoryExt;
        let vertex_buffer = factory.create_vertex_buffer(&FULLSCREEN_QUAD);

        let mut digits = Vec::new();
        let mut alt_digits = Vec::new();

        // just to make the following code a bit less painful
        let mut qpic_to_bitmap = |name: &str| -> Result<BitmapTexture, Error> {
            let qpic = gfx_wad.open_qpic(name)?;
            BitmapTexture::from_qpic(factory, &qpic, palette)
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
            "SHOTGUN",
            "SSHOTGUN",
            "NAILGUN",
            "SNAILGUN",
            "RLAUNCH",
            "SRLAUNCH",
            "LIGHTNG"
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
            digits: digits.into_boxed_slice(),
            minus,
            alt_digits: alt_digits.into_boxed_slice(),
            alt_minus,
            colon,
            slash,

            weapons: weapons.into_boxed_slice(),
            ammo: ammo.into_boxed_slice(),
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
        pso: &PipelineState<Resources, <pipeline2d::Data<Resources> as PipelineData<Resources>>::Meta>,
        user_data: &mut pipeline2d::Data<Resources>,
        display_width: u32,
        display_height: u32,
        position_x: i32,
        position_y: i32,
    )
    where
        C: CommandBuffer<Resources>,
    {
        user_data.vertex_buffer = self.vertex_buffer.clone();
        user_data.transform = bitmap.transform(display_width, display_height, position_x, position_y).into();
        user_data.sampler.0 = bitmap.view();
        encoder.draw(&FULLSCREEN_SLICE, pso, user_data);
    }

    pub fn render<F, C>(
        &mut self,
        factory: &mut F,
        encoder: &mut Encoder<Resources, C>,
        pso: &PipelineState<Resources, <pipeline2d::Data<Resources> as PipelineData<Resources>>::Meta>,
        user_data: &mut pipeline2d::Data<Resources>,
        client: &Client,
        display_width: u32,
        display_height: u32,
    ) -> Result<(), Error>
    where
        F: Factory<Resources>,
        C: CommandBuffer<Resources>,
    {
        // TODO: scale using a cvar (`r_hudscale` or something)
        let display_width = display_width / 2;
        let display_height = display_height / 2;

        let _guard = flame::start_guard("HudRenderer::render");

        let sbar_x = (display_width - self.sbar.width()) as i32 / 2;
        let sbar_y = 0;

        self.render_bitmap(&self.sbar, encoder, pso, user_data, display_width, display_height, sbar_x, sbar_y);
        self.render_bitmap(&self.ibar, encoder, pso, user_data, display_width, display_height, sbar_x, sbar_y + self.ibar.height() as i32);

        // weapons
        for i in 0..8 {
            if client.items().contains(ItemFlags::from_bits(ItemFlags::SHOTGUN.bits() << i).unwrap()) {
                let get_time = client.item_get_time()[i];
                let delta = client.get_time() - get_time;
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
                    pso,
                    user_data,
                    display_width,
                    display_height,
                    sbar_x + 24 * i as i32,
                    sbar_y + self.sbar.height() as i32
                );
            }
        }

        // ammo
        for i in 0..4 {
            let ammo_str = format!("{: >3}", client.stats()[ClientStat::Shells as usize + i]);
            for (chr_id, chr) in ammo_str.chars().enumerate() {
                if chr != ' ' {
                    // TODO
                }
            }
        }

        Ok(())
    }
}
