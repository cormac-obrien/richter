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
use std::collections::HashMap;
use std::ops::DerefMut;
use std::rc::Rc;

use client::menu::Menu;
use client::render::bitmap::BitmapTexture;
use client::render::pipeline2d;
use client::render::{self, GraphicsPackage, Vertex2d};
use common::vfs::Vfs;
use common::wad::QPic;

use failure::Error;
use gfx::handle::Buffer;
use gfx::pso::{PipelineData, PipelineState};
use gfx::traits::FactoryExt;
use gfx::{CommandBuffer, Encoder, Slice};
use gfx_device_gl::Resources;

pub struct MenuRenderer {
    menu: Rc<RefCell<Menu>>,
    gfx_pkg: Rc<RefCell<GraphicsPackage>>,
    vertex_buffer: Buffer<Resources, Vertex2d>,
    slice: Slice<Resources>,

    plaque: BitmapTexture,
    cursor: Vec<BitmapTexture>, // fixed length but dynamically initialized
    tex_cache: RefCell<HashMap<String, BitmapTexture>>,
}

impl MenuRenderer {
    pub fn new(
        vfs: &Vfs,
        menu: Rc<RefCell<Menu>>,
        gfx_pkg: Rc<RefCell<GraphicsPackage>>,
    ) -> Result<MenuRenderer, Error> {
        let vertex_buffer = {
            let pkg_mut = gfx_pkg.borrow_mut();
            let mut factory_mut = pkg_mut.factory_mut();
            factory_mut.create_vertex_buffer(&render::QUAD_VERTICES)
        };

        let slice = Slice::new_match_vertex_buffer(&vertex_buffer);
        println!("loading plaque");
        let plaque = {
            let pkg_mut = gfx_pkg.borrow_mut();
            let tex = BitmapTexture::from_qpic(
                pkg_mut.factory_mut().deref_mut(),
                &QPic::load(vfs.open("gfx/qplaque.lmp").unwrap())?,
                pkg_mut.palette(),
            )?;
            tex
        };

        println!("loading cursor");
        let mut cursor = Vec::new();
        {
            let pkg_mut = gfx_pkg.borrow_mut();
            // let mut factory_mut = pkg_mut.factory_mut();
            for i in 1..=6 {
                cursor.push(BitmapTexture::from_qpic(
                    pkg_mut.factory_mut().deref_mut(),
                    &QPic::load(vfs.open(format!("gfx/menudot{}.lmp", i)).unwrap())?,
                    pkg_mut.palette(),
                )?);
            }
        }

        let tex_cache = RefCell::new(HashMap::new());

        let mut _tex_names = vec![
            "gfx/box_tl.lmp",
            "gfx/box_ml.lmp",
            "gfx/box_bl.lmp",
            "gfx/box_tm.lmp",
            "gfx/box_mm.lmp",
            "gfx/box_mm2.lmp",
            "gfx/box_bm.lmp",
            "gfx/box_tr.lmp",
            "gfx/box_mr.lmp",
            "gfx/box_br.lmp",
            "gfx/ttl_main.lmp",
        ];

        // TODO: walk the entire menu and load all needed textures

        Ok(MenuRenderer {
            menu,
            gfx_pkg,
            vertex_buffer,
            slice,
            plaque,
            cursor,
            tex_cache,
        })
    }

    pub fn render<C>(
        &self,
        encoder: &mut Encoder<Resources, C>,
        pso: &PipelineState<
            Resources,
            <pipeline2d::Data<Resources> as PipelineData<Resources>>::Meta,
        >,
        user_data: &mut pipeline2d::Data<Resources>,
        display_width: u32,
        display_height: u32,
        alpha: f32,
    ) -> Result<(), Error>
    where
        C: CommandBuffer<Resources>,
    {
        ensure!(
            alpha >= 0.0 && alpha <= 1.0,
            "alpha must be between 0 and 1"
        );

        // TODO: replace with cvar scr_conscale
        let display_width = display_width / 2;
        let display_height = display_height / 2;

        // draw Quake plaque
        user_data.vertex_buffer = self.vertex_buffer.clone();
        user_data.transform = render::screen_space_vertex_transform(
            display_width,
            display_height,
            self.plaque.width(),
            self.plaque.height(),
            0,
            0,
        )
        .into();
        user_data.sampler.0 = self.plaque.view();
        encoder.draw(&self.slice, pso, user_data);

        Ok(())
    }
}
