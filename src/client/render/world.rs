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

use client::render::{Camera, ColorFormat, DepthFormat, Palette};
use client::render::brush::{self, BrushPipelineData, BrushPipelineState, BrushVertex, pipe_brush};
use common::bsp::{BspData, BspFace, BspModel, BspTexInfo, BspTextureMipmap};

use cgmath::{Deg, Euler, InnerSpace, Vector3, Matrix4, SquareMatrix};
use chrono::Duration;
use failure::Error;
use gfx::{self, CommandBuffer, Encoder, Factory, IndexBuffer, Slice};
use gfx::format::{R8, Unorm};
use gfx::handle::{Buffer, DepthStencilView, RenderTargetView, Sampler, ShaderResourceView};
use gfx::texture;
use gfx::traits::FactoryExt;
use gfx_device_gl::Resources;

pub struct WorldRenderFace {
    pub slice: Slice<Resources>,
    pub tex_id: usize,
    pub lightmap_id: Option<usize>,
    pub light_styles: [u8; 4],
}

pub struct WorldRenderLeaf {
    pub faces: Box<[WorldRenderFace]>,
}

pub struct WorldRenderer {
    bsp_data: Rc<BspData>,

    leaves: Box<[WorldRenderLeaf]>,
    texture_views: Box<[ShaderResourceView<Resources, [f32; 4]>]>,
    lightmap_views: Box<[ShaderResourceView<Resources, f32>]>,

    pipeline_state: BrushPipelineState,
    vertex_buffer: Buffer<Resources, BrushVertex>,
    dummy_texture: ShaderResourceView<Resources, [f32; 4]>,
    dummy_lightmap: ShaderResourceView<Resources, f32>,

    diffuse_sampler: Sampler<Resources>,
    lightmap_sampler: Sampler<Resources>,
    color_target: RenderTargetView<Resources, ColorFormat>,
    depth_target: DepthStencilView<Resources, DepthFormat>,
}

// FIXME: this calculation is (very slightly) off. not sure why.
fn calculate_lightmap_texcoords(
    position: Vector3<f32>,
    face: &BspFace,
    texinfo: &BspTexInfo,
) -> [f32; 2] {
    let mut s = texinfo.s_vector.dot(position) + texinfo.s_offset;
    s -= face.texture_mins[0] as f32;
    s /= face.extents[0] as f32;

    let mut t = texinfo.t_vector.dot(position) + texinfo.t_offset;
    t -= face.texture_mins[1] as f32;
    t /= face.extents[1] as f32;
    [s, t]
}

impl WorldRenderer {
    pub fn new<F>(
        bsp_model: &BspModel,
        palette: &Palette,
        factory: &mut F,
        color_target: RenderTargetView<Resources, ColorFormat>,
        depth_target: DepthStencilView<Resources, DepthFormat>,
    ) -> Result<WorldRenderer, Error>
    where
        F: Factory<Resources>,
    {
        let mut leaves = Vec::new();
        let mut vertices = Vec::new();
        let mut lightmap_views = Vec::new();

        let pipeline_state = brush::create_pipeline_state(factory)?;

        let bsp_data = bsp_model.bsp_data().clone();

        // BSP vertex data is stored in triangle fan layout so we have to convert to triangle list
        for leaf_id in bsp_model.leaf_id..bsp_model.leaf_id + bsp_model.leaf_count + 1 {
            let mut faces = Vec::new();
            let leaf = &bsp_data.leaves()[leaf_id];
            for facelist_id in leaf.facelist_id..leaf.facelist_id + leaf.facelist_count {
                let face_id = bsp_data.facelist()[facelist_id];
                let face = &bsp_data.faces()[face_id];

                let face_vertex_id = vertices.len();

                let texinfo = &bsp_data.texinfo()[face.texinfo_id];
                let tex = &bsp_data.textures()[texinfo.tex_id];

                let face_edge_ids = &bsp_data.edgelist()[face.edge_id..face.edge_id + face.edge_count];

                let base_edge_id = &face_edge_ids[0];
                let base_vertex_id =
                    bsp_data.edges()[base_edge_id.index].vertex_ids[base_edge_id.direction as usize];
                let base_position = bsp_data.vertices()[base_vertex_id as usize];
                let base_diffuse_s =
                    (base_position.dot(texinfo.s_vector) + texinfo.s_offset) / tex.width() as f32;
                let base_diffuse_t =
                    (base_position.dot(texinfo.t_vector) + texinfo.t_offset) / tex.height() as f32;

                for i in 1..face_edge_ids.len() - 1 {
                    vertices.push(BrushVertex {
                        position: base_position.into(),
                        diffuse_texcoord: [base_diffuse_s, base_diffuse_t],
                        lightmap_texcoord: calculate_lightmap_texcoords(base_position, face, texinfo),
                    });

                    for v in 0..2 {
                        let edge_id = &face_edge_ids[i + v];
                        let vertex_id =
                            bsp_data.edges()[edge_id.index].vertex_ids[edge_id.direction as usize];
                        let position = bsp_data.vertices()[vertex_id as usize];
                        let diffuse_s =
                            (position.dot(texinfo.s_vector) + texinfo.s_offset) / tex.width() as f32;
                        let diffuse_t =
                            (position.dot(texinfo.t_vector) + texinfo.t_offset) / tex.height() as f32;
                        vertices.push(BrushVertex {
                            position: position.into(),
                            diffuse_texcoord: [diffuse_s, diffuse_t],
                            lightmap_texcoord: calculate_lightmap_texcoords(position, face, texinfo),
                        });
                    }
                }

                let lightmap_w = face.extents[0] / 16 + 1;
                let lightmap_h = face.extents[1] / 16 + 1;
                let face_vertex_count = vertices.len() - face_vertex_id;

                let lightmap_size = lightmap_w * lightmap_h;

                // TODO: check r_fullbright != 0

                let lightmap_id = if !texinfo.special {
                    if let Some(ofs) = face.lightmap_id {
                        let lightmap_data = &bsp_data.lightmaps()[ofs..ofs + lightmap_size as usize];
                        let (_lightmap_handle, lightmap_view) = factory.create_texture_immutable_u8::<(R8, Unorm)>(
                            texture::Kind::D2(lightmap_w as u16, lightmap_h as u16, texture::AaMode::Single),
                            texture::Mipmap::Allocated,
                            &[lightmap_data],
                        ).unwrap();
                        let l_id = lightmap_views.len();
                        lightmap_views.push(lightmap_view);
                        Some(l_id)
                    } else {
                        None
                    }
                } else {
                    None
                };

                faces.push(WorldRenderFace {
                    slice: Slice {
                        start: 0,
                        end: face_vertex_count as u32,
                        base_vertex: face_vertex_id as u32,
                        instances: None,
                        buffer: IndexBuffer::Auto,
                    },
                    tex_id: texinfo.tex_id,
                    lightmap_id,
                    light_styles: face.light_styles,
                });
            }

            leaves.push(WorldRenderLeaf {
                faces: faces.into_boxed_slice(),
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

        let (_, dummy_texture) = factory.create_texture_immutable_u8::<ColorFormat>(
            gfx::texture::Kind::D2(0, 0, gfx::texture::AaMode::Single),
            gfx::texture::Mipmap::Allocated,
            &[&[]]
        ).expect("dummy texture generation failed");
        let (_, dummy_lightmap) = factory.create_texture_immutable_u8::<(R8, Unorm)>(
            texture::Kind::D2(1, 1, texture::AaMode::Single),
            texture::Mipmap::Allocated,
            &[&[::std::u8::MAX]],
        ).unwrap();

        Ok(WorldRenderer {
            bsp_data: bsp_data,
            leaves: leaves.into_boxed_slice(),
            pipeline_state,
            vertex_buffer,
            texture_views: texture_views.into_boxed_slice(),
            lightmap_views: lightmap_views.into_boxed_slice(),
            dummy_texture,
            dummy_lightmap,
            diffuse_sampler: factory.create_sampler(gfx::texture::SamplerInfo::new(
                gfx::texture::FilterMethod::Scale,
                gfx::texture::WrapMode::Tile,
            )),
            lightmap_sampler: factory.create_sampler(gfx::texture::SamplerInfo::new(
                gfx::texture::FilterMethod::Bilinear,
                // gfx::texture::FilterMethod::Scale,
                gfx::texture::WrapMode::Tile,
            )),
            color_target,
            depth_target,
        })
    }

    fn create_pipeline_data(&self) -> Result<BrushPipelineData, Error>
    {
        let pipeline_data = pipe_brush::Data {
            vertex_buffer: self.vertex_buffer.clone(),
            transform: Matrix4::identity().into(),
            diffuse_sampler: (self.dummy_texture.clone(), self.diffuse_sampler.clone()),
            lightmap_sampler: (self.dummy_lightmap.clone(), self.lightmap_sampler.clone()),
            lightstyle_value: [0.0; 4],
            out_color: self.color_target.clone(),
            out_depth: self.depth_target.clone(),
        };

        Ok(pipeline_data)
    }

    pub fn render_leaf<C>(
        &self,
        encoder: &mut Encoder<Resources, C>,
        pipeline_state: &BrushPipelineState,
        pipeline_data: &mut BrushPipelineData,
        time: Duration,
        camera: &Camera,
        origin: Vector3<f32>,
        angles: Vector3<Deg<f32>>,
        lightstyle_values: &[f32],
        leaf_id: usize,
    ) where
        C: CommandBuffer<Resources>,
    {
        if leaf_id >= self.leaves.len() {
            error!("leaf ID is out of bounds: the len is {} but the leaf ID is {}", self.leaves.len(), leaf_id);
            return;
        }

        for face in self.leaves[leaf_id].faces.iter() {
            let frame = self.bsp_data.texture_frame_for_time(face.tex_id, time);

            let model_transform = Matrix4::from(Euler::new(angles.x, angles.y, angles.z))
                * Matrix4::from_translation(Vector3::new(-origin.y, origin.z, -origin.x));
            pipeline_data.vertex_buffer = self.vertex_buffer.clone();
            pipeline_data.transform = (camera.get_transform() * model_transform).into();

            pipeline_data.diffuse_sampler.0 = self.texture_views[frame].clone();
            pipeline_data.lightmap_sampler.0 = match face.lightmap_id {
                Some(l_id) => self.lightmap_views[l_id].clone(),
                None => self.dummy_lightmap.clone(),
            };

            let mut lightstyle_value = [-1.0; 4];
            for i in 0..4 {
                if let Some(l) = lightstyle_values.get(face.light_styles[i] as usize) {
                    lightstyle_value[i] = *l;
                }
            }
            pipeline_data.lightstyle_value = lightstyle_value;

            encoder.draw(&face.slice, pipeline_state, pipeline_data);
        }
    }

    #[flame]
    pub fn render<C>(
        &self,
        encoder: &mut Encoder<Resources, C>,
        time: Duration,
        camera: &Camera,
        origin: Vector3<f32>,
        angles: Vector3<Deg<f32>>,
        lightstyle_values: &[f32],
    ) -> Result<(), Error>
    where
        C: CommandBuffer<Resources>,
    {
        let mut pipeline_data = self.create_pipeline_data()?;

        let containing_leaf_id = self.bsp_data.find_leaf(camera.get_origin());
        let pvs = self.bsp_data.get_pvs(containing_leaf_id, self.leaves.len());

        if pvs.is_empty() {
            // No visibility data for this leaf, render all faces
            for leaf_id in 0..self.leaves.len() {
                self.render_leaf(
                    encoder,
                    &self.pipeline_state,
                    &mut pipeline_data,
                    time,
                    camera,
                    origin,
                    angles,
                    lightstyle_values,
                    leaf_id,
                );
            }
        } else {
            for leaf_id in pvs.iter() {
                self.render_leaf(
                    encoder,
                    &self.pipeline_state,
                    &mut pipeline_data,
                    time,
                    camera,
                    origin,
                    angles,
                    lightstyle_values,
                    *leaf_id,
                );
            }
        }

        Ok(())
    }
}
