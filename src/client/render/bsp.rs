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

use cgmath::Euler;
use cgmath::InnerSpace;
use cgmath::Rad;
use cgmath::Vector3;
use cgmath::Matrix4;
use chrono::Duration;
use gfx;
use gfx::CommandBuffer;
use gfx::IndexBuffer;
use gfx::Encoder;
use gfx::Factory;
use gfx::Resources;
use gfx::Slice;
use gfx::format::Srgba8 as ColorFormat;
use gfx::format::DepthStencil as DepthFormat;
use gfx::handle::Buffer;
use gfx::handle::ShaderResourceView;
use gfx::pso::PipelineData;
use gfx::pso::PipelineState;
use gfx::texture;
use gfx::traits::FactoryExt;

pub static BSP_VERTEX_SHADER_GLSL: &[u8] = br#"
#version 430

layout (location = 0) in vec3 a_Pos;
layout (location = 1) in vec2 a_Texcoord;

out vec2 f_texcoord;

uniform mat4 u_Transform;

void main() {
    f_texcoord = a_Texcoord;
    gl_Position = u_Transform * vec4(-a_Pos.y, a_Pos.z, -a_Pos.x, 1.0);
}
"#;

pub static BSP_FRAGMENT_SHADER_GLSL: &[u8] = br#"
#version 430

in vec2 f_texcoord;

uniform sampler2D u_Texture;

out vec4 Target0;

void main() {
    Target0 = texture(u_Texture, f_texcoord);
}"#;

gfx_defines! {
    constant Locals {
        transform: [[f32; 4]; 4] = "u_Transform",
    }

    pipeline pipe {
        vertex_buffer: gfx::VertexBuffer<Vertex> = (),
        transform: gfx::Global<[[f32; 4]; 4]> = "u_Transform",
        sampler: gfx::TextureSampler<[f32; 4]> = "u_Texture",
        out_color: gfx::RenderTarget<ColorFormat> = "Target0",
        out_depth: gfx::DepthTarget<DepthFormat> = gfx::preset::depth::LESS_EQUAL_WRITE,
    }
}

pub struct BspRenderFace<R>
where
    R: Resources,
{
    pub slice: Slice<R>,
    pub tex_id: usize,
}

pub struct BspRenderer<R>
where
    R: Resources,
{
    bsp_data: Rc<BspData>,
    vertex_buffer: Buffer<R, Vertex>,
    faces: Box<[BspRenderFace<R>]>,
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

    pub fn faces(&self) -> &[BspRenderFace<R>] {
        &self.faces
    }

    pub fn get_face(&self, face_id: usize) -> &BspRenderFace<R> {
        &self.faces[face_id]
    }

    pub fn get_texture_view(&self, tex_id: usize) -> ShaderResourceView<R, [f32; 4]> {
        self.texture_views[tex_id].clone()
    }

    pub fn render_face<C>(
        &self,
        encoder: &mut Encoder<R, C>,
        pso: &PipelineState<R, <pipe::Data<R> as PipelineData<R>>::Meta>,
        user_data: &mut pipe::Data<R>,
        time: Duration,
        perspective: Matrix4<f32>,
        camera_pos: Vector3<f32>,
        camera_angles: Euler<Rad<f32>>,
        face_id: usize,
    ) where
        C: CommandBuffer<R>,
    {
        let face = &self.faces[face_id];
        let frame = self.bsp_data.texture_frame_for_time(face.tex_id, time);
        user_data.transform = (perspective * Matrix4::from(camera_angles)
            * Matrix4::from_translation(camera_pos))
            .into();

        user_data.sampler.0 = self.get_texture_view(frame);
        encoder.draw(&face.slice, pso, user_data);
    }

    pub fn render<C>(
        &self,
        encoder: &mut Encoder<R, C>,
        pso: &PipelineState<R, <pipe::Data<R> as PipelineData<R>>::Meta>,
        user_data: &mut pipe::Data<R>,
        time: Duration,
        perspective: Matrix4<f32>,
        camera_pos: Vector3<f32>,
        camera_angles: Euler<Rad<f32>>,
    ) where
        C: CommandBuffer<R>,
    {
        let containing_leaf_id = self.bsp_data.find_leaf(camera_pos);
        debug!("Containing leaf: {}", containing_leaf_id);
        let pvs = self.bsp_data.get_pvs(containing_leaf_id);

        if pvs.is_empty() {
            // No visibility data for this leaf, render all leaves
            for leaf in self.bsp_data.leaves().iter() {
                for facelist_id in leaf.facelist_id..leaf.facelist_id + leaf.facelist_count {
                    let face_id = self.bsp_data.facelist()[facelist_id];
                    self.render_face(
                        encoder,
                        pso,
                        user_data,
                        time,
                        perspective,
                        camera_pos,
                        camera_angles,
                        face_id,
                    );
                }
            }
        } else {
            for visible_leaf_id in pvs.iter() {
                let leaf = &self.bsp_data.leaves()[*visible_leaf_id];

                for facelist_id in leaf.facelist_id..leaf.facelist_id + leaf.facelist_count {
                    let face_id = self.bsp_data.facelist()[facelist_id];
                    self.render_face(
                        encoder,
                        pso,
                        user_data,
                        time,
                        perspective,
                        camera_pos,
                        camera_angles,
                        face_id,
                    );
                }
            }
        }
    }
}
