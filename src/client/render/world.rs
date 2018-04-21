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
use client::render::brush::{self, BrushPipelineData, BrushPipelineState, BrushRenderFace,
    BrushVertex, pipe_brush};
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

pub struct WorldRenderLeaf {
    pub faces: Box<[BrushRenderFace]>,
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
                faces.push(brush::create_brush_render_face(
                    factory,
                    &bsp_data,
                    face_id,
                    &mut vertices,
                    &mut lightmap_views
                )?);
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
