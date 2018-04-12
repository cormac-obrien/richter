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
use client::render::pipe;
use client::render::Vertex;
use common::wad::QPic;
use common::wad::Wad;

use gfx;
use gfx::CommandBuffer;
use gfx::Encoder;
use gfx::Factory;
use gfx::Slice;
use gfx::handle::ShaderResourceView;
use gfx::traits::FactoryExt;
use gfx_device_gl::Resources;

use failure::Error;

struct Texture {
    width: u32,
    height: u32,
    view: ShaderResourceView<Resources, [f32; 4]>,
}

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
    digits: Box<[Texture]>,
    minus: Texture,
    alt_digits: Box<[Texture]>,
    alt_minus: Texture,
    colon: Texture,
    slash: Texture,

    weapons: Box<[Box<[Texture]>]>,
    ammo: Box<[Texture]>,
    armor: Box<[Texture]>,
    items: Box<[Texture]>,
    faces: Box<[Texture]>,
    pain_faces: Box<[Texture]>,

    face_invis: Texture,
    face_invuln: Texture,
    face_invis_invuln: Texture,
    face_quad: Texture,
    sbar: Texture,
    ibar: Texture,
    scorebar: Texture,
}

impl HudRenderer {
    pub fn new<F>(gfx_wad: &Wad, palette: &Palette, factory: &mut F) -> Result<HudRenderer, Error>
    where
        F: Factory<Resources>
    {
        let mut digits = Vec::new();
        let mut alt_digits = Vec::new();

        // just to make the following code a bit less painful
        let mut qpic_to_texture = |name: &str| -> Result<Texture, Error> {
            let qpic = gfx_wad.open_qpic(name)?;
            let rgba = palette.indexed_to_rgba(qpic.indices());

            let (_, view) = factory.create_texture_immutable_u8::<ColorFormat>(
                gfx::texture::Kind::D2(qpic.width() as u16, qpic.height() as u16, gfx::texture::AaMode::Single),
                gfx::texture::Mipmap::Allocated,
                &[&rgba],
            )?;

            Ok(Texture {
                width: qpic.width(),
                height: qpic.height(),
                view,
            })
        };

        for i in 0..10 {
            digits.push(qpic_to_texture(&format!("num_{}", i))?);
            alt_digits.push(qpic_to_texture(&format!("anum_{}", i))?);
        }

        let minus = qpic_to_texture("num_minus")?;
        let alt_minus = qpic_to_texture("anum_minus")?;
        let colon = qpic_to_texture("num_colon")?;
        let slash = qpic_to_texture("num_slash")?;

        let weapon_names = vec![
            "shotgun",
            "sshotgun",
            "nailgun",
            "snailgun",
            "rlaunch",
            "srlaunch",
            "lightng"
        ];

        let weapon_prefixes = vec!["", "2", "a1", "a2", "a3", "a4", "a5"];

        let mut weapons = Vec::new();

        for w_name in &weapon_names {
            let mut weapon_frames = Vec::new();

            for w_prefix in &weapon_prefixes {
                weapon_frames.push(qpic_to_texture(&format!("inv{}_{}", w_prefix, w_name))?);
            }

            weapons.push(weapon_frames.into_boxed_slice());
        }

        let ammo_names = vec!["shells", "nails", "rockets", "cells"];
        let mut ammo = Vec::new();
        for ammo_name in &ammo_names {
            ammo.push(qpic_to_texture(&format!("sb_{}", ammo_name))?);
        }

        let mut armor = Vec::new();
        for i in 1..4 {
            armor.push(qpic_to_texture(&format!("sb_armor{}", i))?);
        }

        let item_names = vec!["key1", "key2", "invis", "invuln", "suit", "quad"];
        let mut items = Vec::new();
        for item_name in &item_names {
            items.push(qpic_to_texture(&format!("sb_{}", item_name))?);
        }

        let mut sigils = Vec::new();
        for i in 1..5 {
            sigils.push(qpic_to_texture(&format!("sb_sigil{}", i))?);
        }

        let mut faces = Vec::new();
        let mut pain_faces = Vec::new();
        for i in 1..6 {
            faces.push(qpic_to_texture(&format!("face{}", i))?);
            pain_faces.push(qpic_to_texture(&format!("face_p{}", i))?);
        }

        let face_invis = qpic_to_texture("face_invis")?;
        let face_invuln = qpic_to_texture("face_invul2")?;
        let face_invis_invuln = qpic_to_texture("face_inv2")?;
        let face_quad = qpic_to_texture("face_quad")?;

        let sbar = qpic_to_texture("sbar")?;
        let ibar = qpic_to_texture("ibar")?;
        let scorebar = qpic_to_texture("scorebar")?;

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
        })
    }
}
