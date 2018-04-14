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

use client::render::Palette;
use client::render::Vertex;
use client::render::bitmap::Bitmap;
use client::render::pipe;
use common::net::ItemFlags;
use common::wad::Wad;

use cgmath::Matrix4;
use cgmath::SquareMatrix;
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

// TODO: make a separate pipeline that doesn't need modified coordinates
static FULLSCREEN_QUAD: [Vertex; 6] = [
    Vertex { pos: [0.0, 1.0, 1.0], texcoord: [0.0, 0.0] },
    Vertex { pos: [0.0, -1.0, 1.0], texcoord: [1.0, 0.0] },
    Vertex { pos: [0.0, 1.0, -1.0], texcoord: [0.0, 1.0] },
    Vertex { pos: [0.0, 1.0, -1.0], texcoord: [0.0, 1.0] },
    Vertex { pos: [0.0, -1.0, 1.0], texcoord: [1.0, 0.0] },
    Vertex { pos: [0.0, -1.0, -1.0], texcoord: [1.0, 1.0] },
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
    digits: Box<[Bitmap]>,
    minus: Bitmap,
    alt_digits: Box<[Bitmap]>,
    alt_minus: Bitmap,
    colon: Bitmap,
    slash: Bitmap,

    weapons: Box<[Box<[Bitmap]>]>,
    ammo: Box<[Bitmap]>,
    armor: Box<[Bitmap]>,
    items: Box<[Bitmap]>,
    faces: Box<[Bitmap]>,
    pain_faces: Box<[Bitmap]>,

    face_invis: Bitmap,
    face_invuln: Bitmap,
    face_invis_invuln: Bitmap,
    face_quad: Bitmap,
    sbar: Bitmap,
    ibar: Bitmap,
    scorebar: Bitmap,

    display_bitmap: Bitmap,
    display_texture_handle: Texture<Resources, R8_G8_B8_A8>,
    display_texture_view: ShaderResourceView<Resources, [f32; 4]>,
    vertex_buffer: Buffer<Resources, Vertex>,
}

impl HudRenderer {
    pub fn new<F>(
        display_width: u32,
        display_height: u32,
        gfx_wad: &Wad,
        palette: &Palette,
        factory: &mut F
    ) -> Result<HudRenderer, Error>
    where
        F: Factory<Resources>
    {
        let mut digits = Vec::new();
        let mut alt_digits = Vec::new();

        // just to make the following code a bit less painful
        let mut qpic_to_bitmap = |name: &str| -> Result<Bitmap, Error> {
            let qpic = gfx_wad.open_qpic(name)?;
            Bitmap::from_qpic(&qpic, palette)
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
        let display_bitmap = Bitmap::transparent(display_width / 2, display_height / 2)?;
        let (display_texture_handle, display_texture_view) = display_bitmap.create_texture(factory)?;
        use gfx::traits::FactoryExt;
        let vertex_buffer = factory.create_vertex_buffer(&FULLSCREEN_QUAD);

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

            display_bitmap,
            display_texture_handle,
            display_texture_view,
            vertex_buffer,
        })
    }

    pub fn render<F, C>(
        &mut self,
        factory: &mut F,
        encoder: &mut Encoder<Resources, C>,
        pso: &PipelineState<Resources, <pipe::Data<Resources> as PipelineData<Resources>>::Meta>,
        user_data: &mut pipe::Data<Resources>,
        items: ItemFlags,
    ) -> Result<(), Error>
    where
        F: Factory<Resources>,
        C: CommandBuffer<Resources>,
    {
        let _guard = flame::start_guard("HudRenderer::render");

        let mut display_bitmap = self.display_bitmap.clone();
        let display_width = self.display_bitmap.width();
        let display_height = self.display_bitmap.height();

        let sbar_x = (display_width - self.sbar.width()) as i32 / 2;
        let sbar_y = (display_height - self.sbar.height()) as i32;

        display_bitmap.blit(&self.sbar, sbar_x, sbar_y);

        // inventory
        display_bitmap.blit(&self.ibar, sbar_x, sbar_y - self.ibar.height() as i32);

        for i in 0..8 {
            if items.contains(ItemFlags::from_bits(ItemFlags::SHOTGUN.bits() << i).unwrap()) {
                display_bitmap.blit(
                    &self.weapons[i][0],
                    sbar_x + 24 * i as i32,
                    sbar_y - self.weapons[0][0].height() as i32
                );
            }
        }

        let (handle, view) = display_bitmap.create_texture(factory)?;
        mem::replace(&mut self.display_texture_handle, handle);
        mem::replace(&mut self.display_texture_view, view);

        user_data.vertex_buffer = self.vertex_buffer.clone();
        user_data.sampler.0 = self.display_texture_view.clone();
        user_data.transform = Matrix4::identity().into();

        encoder.draw(&FULLSCREEN_SLICE, pso, user_data);

        Ok(())
    }
}
