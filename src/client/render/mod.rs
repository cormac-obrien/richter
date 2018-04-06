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

pub mod bsp;

use std::collections::HashMap;

use client::ClientEntity;
use common::model::Model;
use common::model::ModelKind;
use common::pak::Pak;

use cgmath::Deg;
use cgmath::Euler;
use cgmath::Matrix3;
use cgmath::Matrix4;
use cgmath::Vector3;
use chrono::Duration;
use gfx;
use gfx::pso::PipelineData;
use gfx::pso::PipelineState;
use gfx_device_gl::Factory;
use gfx_device_gl::Resources;

pub use gfx::format::Srgba8 as ColorFormat;
pub use gfx::format::DepthStencil as DepthFormat;

use self::bsp::BspRenderer;

const PALETTE_SIZE: usize = 768;

gfx_defines! {
    vertex Vertex {
        pos: [f32; 3] = "a_Pos",
        texcoord: [f32; 2] = "a_Texcoord",
    }

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

pub struct Camera {
    origin: Vector3<f32>,
    angles: Vector3<Deg<f32>>,
    projection: Matrix4<f32>,

    transform: Matrix4<f32>,
}

impl Camera {
    pub fn new(
        origin: Vector3<f32>,
        angles: Vector3<Deg<f32>>,
        projection: Matrix4<f32>,
    ) -> Camera {
        // negate the camera origin and angles
        // TODO: the OpenGL coordinate conversion is hardcoded here! XXX
        let converted_origin = Vector3::new(-origin.y, origin.z, -origin.x);
        let translation = Matrix4::from_translation(-converted_origin);
        let rotation = Matrix4::from(Euler::new(-angles.x, -angles.y, -angles.z));

        Camera {
            origin,
            angles,
            projection,
            transform: projection * rotation * translation,
        }
    }

    pub fn get_origin(&self) -> Vector3<f32> {
        self.origin
    }

    pub fn get_transform(&self) -> Matrix4<f32> {
        self.transform
    }
}

pub struct SceneRenderer {
    bsp_pipeline: PipelineState<Resources, <pipe::Data<Resources> as PipelineData<Resources>>::Meta>,
    bsp_renderers: HashMap<usize, BspRenderer>,
    // mdl_renderers: ...,
    // spr_renderers: ...,
}

impl SceneRenderer {
    pub fn new(models: &[Model], palette: &Palette, factory: &mut Factory) -> SceneRenderer
    {
        use gfx::traits::FactoryExt;
        let bsp_shader_set = factory.create_shader_set(bsp::BSP_VERTEX_SHADER_GLSL, bsp::BSP_FRAGMENT_SHADER_GLSL).unwrap();

        let rasterizer = gfx::state::Rasterizer {
            front_face: gfx::state::FrontFace::Clockwise,
            cull_face: gfx::state::CullFace::Back,
            method: gfx::state::RasterMethod::Fill,
            offset: None,
            samples: Some(gfx::state::MultiSample),
        };

        let bsp_pipeline = factory.create_pipeline_state(
            &bsp_shader_set,
            gfx::Primitive::TriangleList,
            rasterizer,
            pipe::new(),
        ).unwrap();

        let mut bsp_renderers = HashMap::new();
        for (i, model) in models.iter().enumerate() {
            match *model.kind() {
                ModelKind::Brush(ref bmodel) => {
                    bsp_renderers.insert(i, BspRenderer::new(model.name(), &bmodel, palette, factory));
                }

                // TODO: handle other models here
                _ => (),
            }
        }

        SceneRenderer {
            bsp_pipeline,
            bsp_renderers
        }
    }

    #[flame]
    pub fn render<C>(
        &self,
        encoder: &mut gfx::Encoder<Resources, C>,
        user_data: &mut pipe::Data<Resources>,
        entities: &[ClientEntity],
        time: Duration,
        camera: &Camera,
    ) where
        C: gfx::CommandBuffer<Resources>,
    {
        for (i, ent) in entities.iter().enumerate() {
            if let Some(ref bsp_renderer) = self.bsp_renderers.get(&i) {
                bsp_renderer.render(encoder, &self.bsp_pipeline, user_data, time, camera, ent.get_origin(), ent.get_angles());
            }
        }
    }
}

pub struct Palette {
    rgb: [[u8; 3]; 256],
}

impl Palette {
    pub fn load<S>(pak: &Pak, path: S) -> Palette
    where
        S: AsRef<str>,
    {
        let data = pak.open(path).unwrap();
        assert_eq!(data.len(), PALETTE_SIZE);

        let mut rgb = [[0u8; 3]; 256];

        for color in 0..256 {
            for component in 0..3 {
                rgb[color][component] = data[color * 3 + component];
            }
        }

        Palette { rgb }
    }

    // TODO: this will not render console characters correctly, as they use index 0 (black) to
    // indicate transparency.
    pub fn indexed_to_rgba(&self, indices: &[u8]) -> Vec<u8> {
        let mut rgba = Vec::with_capacity(indices.len() * 4);

        for index in indices {
            match *index {
                0xFF => for i in 0..4 {
                    rgba.push(0);
                },

                _ => {
                    for component in 0..3 {
                        rgba.push(self.rgb[*index as usize][component]);
                    }
                    rgba.push(0xFF);
                }
            }
        }

        rgba
    }
}
