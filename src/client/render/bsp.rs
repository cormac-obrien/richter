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

use std::rc::Rc;

use client::render::Palette;
use client::render::Vertex;
use common::bsp::BspData;
use common::bsp::BspTextureMipmap;

use cgmath::InnerSpace;
use cgmath::Vector3;
use gfx::Factory;
use gfx::Resources;
use gfx::format::Srgba8 as ColorFormat;
use gfx::handle::Buffer;
use gfx::handle::ShaderResourceView;
use gfx::texture;
use gfx::traits::FactoryExt;

pub struct BspRenderFace {
    pub vertex_id: usize,
    pub vertex_count: usize,
    pub tex_id: usize,
}

pub struct BspRenderer<R>
where
    R: Resources,
{
    bsp_data: Rc<BspData>,
    vertex_buffer: Buffer<R, Vertex>,
    faces: Box<[BspRenderFace]>,
    texture_views: Box<[ShaderResourceView<R, [f32; 4]>]>,
}

impl<R> BspRenderer<R>
where
    R: Resources,
{
    pub fn new<F>(bsp_data: Rc<BspData>, palette: &Palette, factory: &mut F) -> BspRenderer<R>
    where
        F: Factory<R>,
    {
        let mut faces = Vec::new();
        let mut vertices = Vec::new();

        // BSP vertex data is stored in triangle fan layout so we have to convert to triangle list
        for face in bsp_data.faces().iter() {
            let face_vertex_id = vertices.len();

            let texinfo = &bsp_data.texinfo()[face.texinfo_id];
            let tex = &bsp_data.textures()[texinfo.tex_id];

            let face_edge_ids = &bsp_data.edgelist()[face.edge_id..face.edge_id + face.edge_count];

            let base_edge_id = &face_edge_ids[0];
            let base_vertex_id =
                bsp_data.edges()[base_edge_id.index].vertex_ids[base_edge_id.direction as usize];
            let base_position = bsp_data.vertices()[base_vertex_id as usize];
            let base_s =
                (base_position.dot(texinfo.s_vector) + texinfo.s_offset) / tex.width() as f32;
            let base_t =
                (base_position.dot(texinfo.t_vector) + texinfo.t_offset) / tex.height() as f32;

            for i in 1..face_edge_ids.len() - 1 {
                vertices.push(Vertex {
                    pos: base_position.into(),
                    texcoord: [base_s, base_t],
                });

                for v in 0..2 {
                    let edge_id = &face_edge_ids[i + v];
                    let vertex_id =
                        bsp_data.edges()[edge_id.index].vertex_ids[edge_id.direction as usize];
                    let position = bsp_data.vertices()[vertex_id as usize];
                    let s =
                        (position.dot(texinfo.s_vector) + texinfo.s_offset) / tex.width() as f32;
                    let t =
                        (position.dot(texinfo.t_vector) + texinfo.t_offset) / tex.height() as f32;
                    vertices.push(Vertex {
                        pos: position.into(),
                        texcoord: [s, t],
                    });
                }
            }

            let face_vertex_count = vertices.len() - face_vertex_id;
            faces.push(BspRenderFace {
                vertex_id: face_vertex_id,
                vertex_count: face_vertex_count,
                tex_id: bsp_data.texinfo()[face.texinfo_id].tex_id,
            });
        }

        let vertex_buffer = factory.create_vertex_buffer(&vertices);

        let mut texture_views = Vec::new();
        for tex in bsp_data.textures().iter() {
            let mipmap_full = palette.indexed_to_rgba(tex.mipmap(BspTextureMipmap::Full));
            let (width, height) = tex.dimensions();

            let (_, view) = factory
                .create_texture_immutable_u8::<ColorFormat>(
                    texture::Kind::D2(width as u16, height as u16, texture::AaMode::Single),
                    texture::Mipmap::Provided,
                    &[&mipmap_full],
                )
                .unwrap();

            texture_views.push(view);
        }

        BspRenderer {
            bsp_data,
            vertex_buffer,
            faces: faces.into_boxed_slice(),
            texture_views: texture_views.into_boxed_slice(),
        }
    }

    pub fn vertex_buffer(&self) -> Buffer<R, Vertex> {
        self.vertex_buffer.clone()
    }

    pub fn faces(&self) -> &[BspRenderFace] {
        &self.faces
    }

    pub fn get_face(&self, face_id: usize) -> &BspRenderFace {
        &self.faces[face_id]
    }

    pub fn get_texture_view(&self, tex_id: usize) -> ShaderResourceView<R, [f32; 4]> {
        self.texture_views[tex_id].clone()
    }
}
