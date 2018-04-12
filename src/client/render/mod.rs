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

pub mod alias;
pub mod bitmap;
pub mod brush;
pub mod hud;
pub mod world;

use std::collections::HashMap;

use client::ClientEntity;
use common::model::Model;
use common::model::ModelKind;
use common::pak::Pak;

use cgmath::Deg;
use cgmath::Euler;
use cgmath::Matrix4;
use cgmath::Vector3;
use cgmath::Zero;
use chrono::Duration;
use failure::Error;
use gfx;
use gfx::pso::PipelineData;
use gfx::pso::PipelineState;
use gfx_device_gl::Factory;
use gfx_device_gl::Resources;

pub use gfx::format::Srgba8 as ColorFormat;
pub use gfx::format::DepthStencil as DepthFormat;

use self::alias::AliasRenderer;
use self::brush::BrushRenderer;
use self::world::WorldRenderer;

const PALETTE_SIZE: usize = 768;

pub static VERTEX_SHADER_GLSL: &[u8] = br#"
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

pub static FRAGMENT_SHADER_GLSL: &[u8] = br#"
#version 430

in vec2 f_texcoord;

uniform sampler2D u_Texture;

out vec4 Target0;

void main() {
    Target0 = texture(u_Texture, f_texcoord);
}"#;

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
    pipeline: PipelineState<Resources, <pipe::Data<Resources> as PipelineData<Resources>>::Meta>,
    world_renderer: WorldRenderer,
    brush_renderers: HashMap<usize, BrushRenderer>,
    alias_renderers: HashMap<usize, AliasRenderer>,
    // spr_renderers: ...,
}

impl SceneRenderer {
    pub fn new(models: &[Model], worldmodel_id: usize, palette: &Palette, factory: &mut Factory) -> Result<SceneRenderer, Error>
    {
        use gfx::traits::FactoryExt;
        let shader_set = factory.create_shader_set(VERTEX_SHADER_GLSL, FRAGMENT_SHADER_GLSL).unwrap();

        let rasterizer = gfx::state::Rasterizer {
            front_face: gfx::state::FrontFace::Clockwise,
            cull_face: gfx::state::CullFace::Back,
            method: gfx::state::RasterMethod::Fill,
            offset: None,
            samples: Some(gfx::state::MultiSample),
        };

        let pipeline = factory.create_pipeline_state(
            &shader_set,
            gfx::Primitive::TriangleList,
            rasterizer,
            pipe::new(),
        ).unwrap();

        let mut maybe_world_renderer = None;
        let mut brush_renderers = HashMap::new();
        let mut alias_renderers = HashMap::new();
        for (i, model) in models.iter().enumerate() {
            if i == worldmodel_id {
                match *model.kind() {
                    ModelKind::Brush(ref bmodel) => {
                        debug!("model {}: world model", i);
                        maybe_world_renderer = Some(WorldRenderer::new(model.name(), &bmodel, palette, factory));
                    }

                    _ => bail!("Invalid kind for worldmodel"),
                }
            } else {
                match *model.kind() {
                    ModelKind::Brush(ref bmodel) => {
                        debug!("model {}: brush model", i);
                        brush_renderers.insert(i, BrushRenderer::new(model.name(), &bmodel, palette, factory));
                    }

                    ModelKind::Alias(ref amodel) => {
                        debug!("model {}: alias model", i);
                        alias_renderers.insert(i, AliasRenderer::new(&amodel, palette, factory)?);
                    },

                    // TODO handle sprite and null models
                    ModelKind::Sprite(_) => debug!("model {}: sprite model", i),
                    _ => (),
                }
            }
        }

        let world_renderer = match maybe_world_renderer {
            Some(w) => w,
            None => bail!("No worldmodel provided"),
        };

        Ok(SceneRenderer {
            pipeline,
            world_renderer,
            brush_renderers,
            alias_renderers
        })
    }

    #[flame]
    pub fn render<C>(
        &self,
        encoder: &mut gfx::Encoder<Resources, C>,
        user_data: &mut pipe::Data<Resources>,
        entities: &[ClientEntity],
        time: Duration,
        camera: &Camera,
    ) -> Result<(), Error>
    where
        C: gfx::CommandBuffer<Resources>,
    {
        self.world_renderer.render(
            encoder,
            &self.pipeline,
            user_data,
            time,
            camera,
            Vector3::zero(),
            Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
        );

        for ent in entities.iter() {
            let model_id = ent.get_model_id();
            if let Some(ref brush_renderer) = self.brush_renderers.get(&model_id) {
                brush_renderer.render(
                    encoder,
                    &self.pipeline,
                    user_data,
                    time,
                    camera,
                    ent.get_origin(),
                    ent.get_angles()
                );
            } else if let Some(ref alias_renderer) = self.alias_renderers.get(&model_id) {
                // TODO: pull keyframe and texture ID
                alias_renderer.render(
                    encoder,
                    &self.pipeline,
                    user_data,
                    time,
                    camera,
                    ent.get_origin(),
                    ent.get_angles(),
                    0,
                    0
                )?;
            }
        }

        Ok(())
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
