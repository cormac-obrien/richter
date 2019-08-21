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

use crate::client::render::Camera;
use crate::client::render::ColorFormat;
use crate::client::render::Palette;
use crate::client::render::Vertex;
use crate::client::render::pipe;
use crate::common::mdl::AliasModel;
use crate::common::mdl::Keyframe;
use crate::common::mdl::Texture;

use cgmath::Deg;
use cgmath::Euler;
use cgmath::Matrix4;
use cgmath::Vector3;
use chrono::Duration;
use failure::Error;
use gfx;
use gfx::CommandBuffer;
use gfx::Encoder;
use gfx::Factory;
use gfx::IndexBuffer;
use gfx::Slice;
use gfx::handle::Buffer;
use gfx::handle::ShaderResourceView;
use gfx::pso::PipelineData;
use gfx::pso::PipelineState;
use gfx_device_gl::Resources;

pub struct AliasRenderStaticTexture {
    view: ShaderResourceView<Resources, [f32; 4]>,
}

pub struct AliasRenderAnimatedTexture {
    total_duration: Duration,
    durations: Box<[Duration]>,
    views: Box<[ShaderResourceView<Resources, [f32; 4]>]>
}

pub enum AliasRenderTexture {
    Static(AliasRenderStaticTexture),
    Animated(AliasRenderAnimatedTexture),
}

pub struct AliasRenderStaticKeyframe {
    slice: Slice<Resources>,
}

pub struct AliasRenderAnimatedKeyframe {
    total_duration: Duration,
    durations: Box<[Duration]>,
    slices: Box<[Slice<Resources>]>
}

pub enum AliasRenderKeyframe {
    Static(AliasRenderStaticKeyframe),
    Animated(AliasRenderAnimatedKeyframe),
}

pub struct AliasRenderer {
    keyframes: Box<[AliasRenderKeyframe]>,
    textures: Box<[AliasRenderTexture]>,
    vertex_buffer: Buffer<Resources, Vertex>,
}

impl AliasRenderer {
    pub fn new<F>(alias_model: &AliasModel, palette: &Palette, factory: &mut F) -> Result<AliasRenderer, Error>
    where
        F: Factory<Resources>
    {
        let w = alias_model.texture_width();
        let h = alias_model.texture_height();

        let mut vertices = Vec::new();
        let mut keyframes = Vec::new();

        for keyframe in alias_model.keyframes() {
            match *keyframe {
                Keyframe::Static(ref static_keyframe) => {
                    let vertex_id = vertices.len();
                    for polygon in alias_model.polygons() {
                        for index in polygon.indices() {
                            let pos = static_keyframe.vertices()[*index as usize];
                            let texcoord = &alias_model.texcoords()[*index as usize];

                            let s = if !polygon.faces_front() && texcoord.is_on_seam() {
                                (texcoord.s() + w / 2) as f32 + 0.5
                            } else {
                                texcoord.s() as f32 + 0.5
                            } / w as f32;

                            let t = (texcoord.t() as f32 + 0.5) / h as f32;
                            vertices.push(Vertex {
                                pos: pos.into(),
                                texcoord: [s, t],
                            });
                        }
                    }

                    let vertex_count = vertices.len() - vertex_id;

                    keyframes.push(AliasRenderKeyframe::Static(AliasRenderStaticKeyframe {
                        slice: Slice {
                            start: 0,
                            end: vertex_count as u32,
                            base_vertex: vertex_id as u32,
                            instances: None,
                            buffer: IndexBuffer::Auto,
                        },
                    }));
                }

                Keyframe::Animated(ref animated_keyframe) => {
                    let mut durations = Vec::new();
                    let mut slices = Vec::new();
                    for frame in animated_keyframe.frames() {
                        durations.push(frame.duration());

                        let vertex_id = vertices.len();
                        for polygon in alias_model.polygons() {
                            for index in polygon.indices() {
                                let pos = frame.vertices()[*index as usize];
                                let texcoord = &alias_model.texcoords()[*index as usize];

                                let s = if !polygon.faces_front() && texcoord.is_on_seam() {
                                    (texcoord.s() + w / 2) as f32 + 0.5
                                } else {
                                    texcoord.s() as f32 + 0.5
                                } / w as f32;

                                let t = (texcoord.t() as f32 + 0.5) / h as f32;
                                vertices.push(Vertex {
                                    pos: pos.into(),
                                    texcoord: [s, t],
                                });
                            }
                        }

                        let vertex_count = vertices.len() - vertex_id;
                        slices.push(Slice {
                            start: 0,
                            end: vertex_count as u32,
                            base_vertex: vertex_id as u32,
                            instances: None,
                            buffer: IndexBuffer::Auto,
                        });
                    }

                    let mut total_duration = Duration::zero();
                    for duration in &durations {
                        total_duration = total_duration + *duration;
                    }

                    keyframes.push(AliasRenderKeyframe::Animated(AliasRenderAnimatedKeyframe {
                        total_duration,
                        durations: durations.into_boxed_slice(),
                        slices: slices.into_boxed_slice(),
                    }));
                }
            }
        }

        use gfx::traits::FactoryExt;
        let vertex_buffer = factory.create_vertex_buffer(&vertices);

        let mut textures = Vec::new();
        for texture in alias_model.textures() {
            match *texture {
                Texture::Static(ref static_texture) => {
                    let (rgba, _fullbright) = palette.translate(static_texture.indices());
                    let (_, view) = factory
                        .create_texture_immutable_u8::<ColorFormat>(
                            gfx::texture::Kind::D2(w as u16, h as u16, gfx::texture::AaMode::Single),
                            gfx::texture::Mipmap::Allocated,
                            &[&rgba],
                        )?;

                    textures.push(AliasRenderTexture::Static(AliasRenderStaticTexture {
                        view,
                    }));
                }

                Texture::Animated(ref animated_texture) => {
                    let mut durations = Vec::new();
                    let mut views = Vec::new();

                    for frame in animated_texture.frames() {
                        durations.push(frame.duration());

                        let (rgba, _fullbright) = palette.translate(frame.indices());
                        let (_, view) = factory
                            .create_texture_immutable_u8::<ColorFormat>(
                                gfx::texture::Kind::D2(w as u16, h as u16, gfx::texture::AaMode::Single),
                                gfx::texture::Mipmap::Allocated,
                                &[&rgba],
                            )?;

                        views.push(view);
                    }

                    let mut total_duration = Duration::zero();
                    for duration in &durations {
                        total_duration = total_duration + *duration;
                    }

                    textures.push(AliasRenderTexture::Animated(AliasRenderAnimatedTexture {
                        total_duration,
                        durations: durations.into_boxed_slice(),
                        views: views.into_boxed_slice(),
                    }));
                }
            }
        }

        Ok(AliasRenderer {
            keyframes: keyframes.into_boxed_slice(),
            textures: textures.into_boxed_slice(),
            vertex_buffer,
        })
    }

    pub fn render<C>(
        &self,
        encoder: &mut Encoder<Resources, C>,
        pso: &PipelineState<Resources, <pipe::Data<Resources> as PipelineData<Resources>>::Meta>,
        user_data: &mut pipe::Data<Resources>,
        time: Duration,
        camera: &Camera,
        origin: Vector3<f32>,
        angles: Vector3<Deg<f32>>,
        keyframe_id: usize,
        texture_id: usize,
    ) -> Result<(), Error>
    where
        C: CommandBuffer<Resources>,
    {
        ensure!(keyframe_id < self.keyframes.len(), "Keyframe ID out of range: {}", keyframe_id);
        ensure!(texture_id < self.textures.len(), "Texture ID out of range: {}", texture_id);

        let model_transform = Matrix4::from_translation(Vector3::new(-origin.y, origin.z, -origin.x))
            * Matrix4::from(Euler::new(angles.x, angles.y, angles.z));

        user_data.vertex_buffer = self.vertex_buffer.clone();
        user_data.transform = (camera.transform() * model_transform).into();

        match self.textures[texture_id] {
            AliasRenderTexture::Static(ref static_texture) => {
                user_data.sampler.0 = static_texture.view.clone();
            }

            AliasRenderTexture::Animated(ref animated_texture) => {
                // pick a fallback texture
                user_data.sampler.0 = animated_texture.views[0].clone();

                let mut time_ms = time.num_milliseconds() % animated_texture.total_duration.num_milliseconds();

                for (frame_id, frame_duration) in animated_texture.durations.iter().enumerate() {
                    time_ms -= frame_duration.num_milliseconds();
                    if time_ms <= 0 {
                        user_data.sampler.0 = animated_texture.views[frame_id].clone();
                        break;
                    }
                }
            }
        }

        match self.keyframes[keyframe_id] {
            AliasRenderKeyframe::Static(ref static_keyframe) => {
                encoder.draw(&static_keyframe.slice, pso, user_data);
            }

            AliasRenderKeyframe::Animated(ref animated_keyframe) => {
                // pick a fallback slice
                let mut slice = &animated_keyframe.slices[0];

                let mut time_ms = time.num_milliseconds() % animated_keyframe.total_duration.num_milliseconds();
                for (frame_id, frame_duration) in animated_keyframe.durations.iter().enumerate() {
                    time_ms -= frame_duration.num_milliseconds();
                    if time_ms <= 0 {
                        slice = &animated_keyframe.slices[frame_id];
                        break;
                    }
                }

                encoder.draw(slice, pso, user_data);
            }
        }

        Ok(())
    }
}
