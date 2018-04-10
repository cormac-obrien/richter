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

use client::render::Camera;
use client::render::Palette;
use client::render::pipe;
use client::render::Vertex;
use common::bsp::BspData;
use common::bsp::BspModel;
use common::bsp::BspTextureMipmap;

use cgmath::Deg;
use cgmath::Euler;
use cgmath::InnerSpace;
use cgmath::Vector3;
use cgmath::Matrix4;
use chrono::Duration;
use gfx::CommandBuffer;
use gfx::Encoder;
use gfx::Factory;
use gfx::IndexBuffer;
use gfx::Slice;
use gfx::format::Srgba8 as ColorFormat;
use gfx::handle::Buffer;
use gfx::handle::ShaderResourceView;
use gfx::pso::PipelineData;
use gfx::pso::PipelineState;
use gfx::texture;
use gfx::traits::FactoryExt;
use gfx_device_gl::Resources;

pub struct BrushRenderFace {
    pub slice: Slice<Resources>,
    pub tex_id: usize,
}

pub struct BrushRenderer {
    model_name: String,
    bsp_data: Rc<BspData>,
    vertex_buffer: Buffer<Resources, Vertex>,
    faces: Box<[BrushRenderFace]>,
    texture_views: Box<[ShaderResourceView<Resources, [f32; 4]>]>,
}

impl BrushRenderer {
    pub fn new<S, F>(model_name: S, bsp_model: &BspModel, palette: &Palette, factory: &mut F) -> BrushRenderer
    where
        S: AsRef<str>,
        F: Factory<Resources>,
    {
        let mut faces = Vec::new();

        let mut vertices = Vec::new();
        let bsp_data = bsp_model.bsp_data().clone();

        // BSP vertex data is stored in triangle fan layout so we have to convert to triangle list
        for face_id in bsp_model.face_id..bsp_model.face_id + bsp_model.face_count {
            let face = &bsp_data.faces()[face_id];

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
            faces.push(BrushRenderFace {
                slice: Slice {
                    start: 0,
                    end: face_vertex_count as u32,
                    base_vertex: face_vertex_id as u32,
                    instances: None,
                    buffer: IndexBuffer::Auto,
                },
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

        BrushRenderer {
            model_name: model_name.as_ref().to_owned(),
            bsp_data: bsp_data,
            vertex_buffer,
            faces: faces.into_boxed_slice(),
            texture_views: texture_views.into_boxed_slice(),
        }
    }

    pub fn vertex_buffer(&self) -> Buffer<Resources, Vertex> {
        self.vertex_buffer.clone()
    }

    pub fn get_texture_view(&self, tex_id: usize) -> ShaderResourceView<Resources, [f32; 4]> {
        self.texture_views[tex_id].clone()
    }

    #[flame]
    pub fn render<C>(
        &self,
        encoder: &mut Encoder<Resources, C>,
        pso: &PipelineState<Resources, <pipe::Data<Resources> as PipelineData<Resources>>::Meta>,
        user_data: &mut pipe::Data<Resources>,
        time: Duration,
        camera: &Camera,
        origin: Vector3<f32>,
        angles: Vector3<Deg<f32>>,
    ) where
        C: CommandBuffer<Resources>,
    {
        for face in self.faces.iter() {
            let frame = self.bsp_data.texture_frame_for_time(face.tex_id, time);

            let model_transform = Matrix4::from(Euler::new(angles.x, angles.y, angles.z))
                * Matrix4::from_translation(Vector3::new(-origin.y, origin.z, -origin.x));
            user_data.vertex_buffer = self.vertex_buffer.clone();
            user_data.transform = (camera.get_transform() * model_transform).into();

            user_data.sampler.0 = self.get_texture_view(frame);
            encoder.draw(&face.slice, pso, user_data);
        }
    }
}
