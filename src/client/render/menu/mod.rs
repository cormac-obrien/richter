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

mod layout;

use std::{cell::RefCell, collections::HashMap};

use std::rc::Rc;

use crate::{
    client::{
        menu::Menu,
        render::{self, bitmap::BitmapTexture, pipeline2d, GraphicsPackage, Vertex2d},
    },
    common::vfs::Vfs,
};

use failure::Error;
use gfx::{
    handle::Buffer,
    pso::{PipelineData, PipelineState},
    traits::FactoryExt,
    CommandBuffer, Encoder, Slice,
};
use gfx_device_gl::Resources;

use self::layout::{Layout, LayoutElement, Position};

const MENU_WIDTH: u32 = 320;

pub struct MenuRenderer {
    vfs: Rc<Vfs>,
    menu: Rc<RefCell<Menu>>,
    layout: Option<Layout>, // TODO: make this non-Optional
    gfx_pkg: Rc<RefCell<GraphicsPackage>>,
    vertex_buffer: Buffer<Resources, Vertex2d>,
    slice: Slice<Resources>,
    tex_cache: RefCell<HashMap<String, Rc<BitmapTexture>>>,
}

impl MenuRenderer {
    pub fn new(
        vfs: Rc<Vfs>,
        menu: Rc<RefCell<Menu>>,
        gfx_pkg: Rc<RefCell<GraphicsPackage>>,
    ) -> Result<MenuRenderer, Error> {
        let vertex_buffer = {
            let pkg_mut = gfx_pkg.borrow_mut();
            let mut factory_mut = pkg_mut.factory_mut();
            factory_mut.create_vertex_buffer(&render::QUAD_VERTICES)
        };

        let slice = Slice::new_match_vertex_buffer(&vertex_buffer);

        let tex_cache = RefCell::new(HashMap::new());

        // TODO: consider precaching textures we know we'll need

        let mut layout = None;
        // if let Some(ref name) = menu.borrow().gfx_name() {
            // layout = Layout::predefined(name);
        // }

        Ok(MenuRenderer {
            vfs,
            menu,
            layout,
            gfx_pkg,
            vertex_buffer,
            slice,
            tex_cache,
        })
    }

    pub fn cache_texture<S>(&self, name: S) -> Rc<BitmapTexture>
    where
        S: AsRef<str>,
    {
        let tex = Rc::new(
            self.gfx_pkg
                .borrow()
                .texture_from_qpic(&self.vfs, name.as_ref()),
        );

        self.tex_cache
            .borrow_mut()
            .insert(name.as_ref().to_owned(), tex.clone());
        tex.clone()
    }

    pub fn texture<S>(&self, name: S) -> Rc<BitmapTexture>
    where
        S: AsRef<str>,
    {
        {
            if let Some(t) = self.tex_cache.borrow().get(name.as_ref()) {
                return t.clone();
            }
        }

        return self.cache_texture(name);
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

        if let Some(ref l) = self.layout {
            for elem in l.elements() {
                match elem {
                    LayoutElement::Bitmap { name, x, y } => {
                        user_data.vertex_buffer = self.vertex_buffer.clone();
                        user_data.transform = render::screen_space_vertex_transform(
                            display_width,
                            display_height,
                            self.texture(name).width(),
                            self.texture(name).height(),
                            position_to_absolute(x),
                            position_to_absolute(y),
                        )
                        .into();
                        user_data.sampler.0 = self.texture(name).view();
                        encoder.draw(&self.slice, pso, user_data);
                    }
                }
            }
        }

        Ok(())
    }
}

fn position_to_absolute(pos: &Position) -> i32 {
    match pos {
        Position::Absolute(x) => *x as i32,
        Position::CenterRelative(x) => (MENU_WIDTH as i32 + *x) / 2,
    }
}
