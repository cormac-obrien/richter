// Copyright © 2020 Cormac O'Brien.
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

/// Rendering functionality.
///
/// # Pipeline stages
///
/// The current rendering implementation consists of the following stages:
/// - Initial geometry pass
///   - Inputs:
///     - `AliasPipeline`
///     - `BrushPipeline`
///     - `SpritePipeline`
///   - Output: `InitialPassTarget`
/// - Deferred lighting pass
///   - Inputs:
///     - `DeferredPipeline`
///   - Output: `DeferredPassTarget`
/// - Final pass
///   - Inputs:
///     - `PostProcessPipeline`
///     - `QuadPipeline`
///     - `GlyphPipeline`
///   - Output: `FinalPassTarget`
/// - Blit to swap chain
///   - Inputs:
///     - `BlitPipeline`
///   - Output: `SwapChainTarget`
// mod atlas;
mod blit;
mod cvars;
mod error;
mod palette;
mod pipeline;
mod target;
mod ui;
mod uniform;
mod warp;
mod world;

pub use cvars::register_cvars;
pub use error::{RenderError, RenderErrorKind};
pub use palette::Palette;
pub use pipeline::Pipeline;
pub use postprocess::PostProcessRenderer;
pub use target::{RenderTarget, RenderTargetResolve, SwapChainTarget};
pub use ui::{hud::HudState, UiOverlay, UiRenderer, UiState};
pub use world::{
    deferred::{DeferredRenderer, DeferredUniforms, PointLight},
    Camera, WorldRenderer,
};

use std::{
    borrow::Cow,
    cell::{Cell, Ref, RefCell, RefMut},
    mem::size_of,
    num::{NonZeroU32, NonZeroU64, NonZeroU8},
    rc::Rc,
};

use crate::{
    client::{
        entity::MAX_LIGHTS,
        input::InputFocus,
        menu::Menu,
        render::{
            blit::BlitPipeline,
            target::{DeferredPassTarget, FinalPassTarget, InitialPassTarget},
            ui::{glyph::GlyphPipeline, quad::QuadPipeline},
            uniform::DynamicUniformBuffer,
            world::{
                alias::AliasPipeline,
                brush::BrushPipeline,
                deferred::DeferredPipeline,
                particle::ParticlePipeline,
                postprocess::{self, PostProcessPipeline},
                sprite::SpritePipeline,
                EntityUniforms,
            },
        },
        Connection, ConnectionKind,
    },
    common::{
        console::{Console, CvarRegistry},
        vfs::Vfs,
        wad::Wad,
    },
};

use super::ConnectionState;
use bumpalo::Bump;
use cgmath::{Deg, Vector3, Zero};
use chrono::{DateTime, Utc};
use failure::Error;

const DEPTH_ATTACHMENT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub const DIFFUSE_ATTACHMENT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;
const NORMAL_ATTACHMENT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const LIGHT_ATTACHMENT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

const DIFFUSE_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const FULLBRIGHT_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
const LIGHTMAP_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

/// Create a `wgpu::TextureDescriptor` appropriate for the provided texture data.
pub fn texture_descriptor(
    label: Option<&str>,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> wgpu::TextureDescriptor {
    wgpu::TextureDescriptor {
        label,
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsage::COPY_DST | wgpu::TextureUsage::SAMPLED,
    }
}

pub fn create_texture<'a>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: Option<&'a str>,
    width: u32,
    height: u32,
    data: &TextureData,
) -> wgpu::Texture {
    trace!(
        "Creating texture ({:?}: {}x{})",
        data.format(),
        width,
        height
    );
    let texture = device.create_texture(&texture_descriptor(label, width, height, data.format()));
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
        },
        data.data(),
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: NonZeroU32::new(width * data.stride()),
            rows_per_image: None,
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    texture
}

pub struct DiffuseData<'a> {
    pub rgba: Cow<'a, [u8]>,
}

pub struct FullbrightData<'a> {
    pub fullbright: Cow<'a, [u8]>,
}

pub struct LightmapData<'a> {
    pub lightmap: Cow<'a, [u8]>,
}

pub enum TextureData<'a> {
    Diffuse(DiffuseData<'a>),
    Fullbright(FullbrightData<'a>),
    Lightmap(LightmapData<'a>),
}

impl<'a> TextureData<'a> {
    pub fn format(&self) -> wgpu::TextureFormat {
        match self {
            TextureData::Diffuse(_) => DIFFUSE_TEXTURE_FORMAT,
            TextureData::Fullbright(_) => FULLBRIGHT_TEXTURE_FORMAT,
            TextureData::Lightmap(_) => LIGHTMAP_TEXTURE_FORMAT,
        }
    }

    pub fn data(&self) -> &[u8] {
        match self {
            TextureData::Diffuse(d) => &d.rgba,
            TextureData::Fullbright(d) => &d.fullbright,
            TextureData::Lightmap(d) => &d.lightmap,
        }
    }

    pub fn stride(&self) -> u32 {
        (match self {
            TextureData::Diffuse(_) => size_of::<[u8; 4]>(),
            TextureData::Fullbright(_) => size_of::<u8>(),
            TextureData::Lightmap(_) => size_of::<u8>(),
        }) as u32
    }

    pub fn size(&self) -> wgpu::BufferAddress {
        self.data().len() as wgpu::BufferAddress
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Extent2d {
    pub width: u32,
    pub height: u32,
}

impl Into<wgpu::Extent3d> for Extent2d {
    fn into(self) -> wgpu::Extent3d {
        wgpu::Extent3d {
            width: self.width,
            height: self.height,
            depth_or_array_layers: 1,
        }
    }
}

impl std::convert::From<winit::dpi::PhysicalSize<u32>> for Extent2d {
    fn from(other: winit::dpi::PhysicalSize<u32>) -> Extent2d {
        let winit::dpi::PhysicalSize { width, height } = other;
        Extent2d { width, height }
    }
}

pub struct GraphicsState {
    device: wgpu::Device,
    queue: wgpu::Queue,

    initial_pass_target: InitialPassTarget,
    deferred_pass_target: DeferredPassTarget,
    final_pass_target: FinalPassTarget,

    world_bind_group_layouts: Vec<wgpu::BindGroupLayout>,
    world_bind_groups: Vec<wgpu::BindGroup>,

    frame_uniform_buffer: wgpu::Buffer,

    entity_uniform_buffer: RefCell<DynamicUniformBuffer<EntityUniforms>>,
    diffuse_sampler: wgpu::Sampler,
    lightmap_sampler: wgpu::Sampler,

    sample_count: Cell<u32>,

    alias_pipeline: AliasPipeline,
    brush_pipeline: BrushPipeline,
    sprite_pipeline: SpritePipeline,
    deferred_pipeline: DeferredPipeline,
    particle_pipeline: ParticlePipeline,
    postprocess_pipeline: PostProcessPipeline,
    glyph_pipeline: GlyphPipeline,
    quad_pipeline: QuadPipeline,
    blit_pipeline: BlitPipeline,

    default_lightmap: wgpu::Texture,
    default_lightmap_view: wgpu::TextureView,

    vfs: Rc<Vfs>,
    palette: Palette,
    gfx_wad: Wad,
    compiler: RefCell<shaderc::Compiler>,
}

impl GraphicsState {
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        size: Extent2d,
        sample_count: u32,
        vfs: Rc<Vfs>,
    ) -> Result<GraphicsState, Error> {
        let palette = Palette::load(&vfs, "gfx/palette.lmp");
        let gfx_wad = Wad::load(vfs.open("gfx.wad")?).unwrap();
        let mut compiler = shaderc::Compiler::new().unwrap();

        let initial_pass_target = InitialPassTarget::new(&device, size, sample_count);
        let deferred_pass_target = DeferredPassTarget::new(&device, size, sample_count);
        let final_pass_target = FinalPassTarget::new(&device, size, sample_count);

        let frame_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame uniform buffer"),
            size: size_of::<world::FrameUniforms>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
            mapped_at_creation: false,
        });
        let entity_uniform_buffer = RefCell::new(DynamicUniformBuffer::new(&device));

        let diffuse_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: None,
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            // TODO: these are the OpenGL defaults; see if there's a better choice for us
            lod_min_clamp: -1000.0,
            lod_max_clamp: 1000.0,
            compare: None,
            anisotropy_clamp: NonZeroU8::new(16),
            ..Default::default()
        });

        let lightmap_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: None,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            // TODO: these are the OpenGL defaults; see if there's a better choice for us
            lod_min_clamp: -1000.0,
            lod_max_clamp: 1000.0,
            compare: None,
            anisotropy_clamp: NonZeroU8::new(16),
            ..Default::default()
        });

        let world_bind_group_layouts: Vec<wgpu::BindGroupLayout> =
            world::BIND_GROUP_LAYOUT_DESCRIPTORS
                .iter()
                .map(|desc| device.create_bind_group_layout(desc))
                .collect();
        let world_bind_groups = vec![
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("per-frame bind group"),
                layout: &world_bind_group_layouts[world::BindGroupLayoutId::PerFrame as usize],
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &frame_uniform_buffer,
                        offset: 0,
                        size: None,
                    }),
                }],
            }),
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("brush per-entity bind group"),
                layout: &world_bind_group_layouts[world::BindGroupLayoutId::PerEntity as usize],
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: entity_uniform_buffer.borrow().buffer(),
                            offset: 0,
                            size: Some(
                                NonZeroU64::new(size_of::<EntityUniforms>() as u64).unwrap(),
                            ),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&diffuse_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&lightmap_sampler),
                    },
                ],
            }),
        ];

        let alias_pipeline = AliasPipeline::new(
            &device,
            &mut compiler,
            &world_bind_group_layouts,
            sample_count,
        );
        let brush_pipeline = BrushPipeline::new(
            &device,
            &queue,
            &mut compiler,
            &world_bind_group_layouts,
            sample_count,
        );
        let sprite_pipeline = SpritePipeline::new(
            &device,
            &mut compiler,
            &world_bind_group_layouts,
            sample_count,
        );
        let deferred_pipeline = DeferredPipeline::new(&device, &mut compiler, sample_count);
        let particle_pipeline =
            ParticlePipeline::new(&device, &queue, &mut compiler, sample_count, &palette);
        let postprocess_pipeline = PostProcessPipeline::new(&device, &mut compiler, sample_count);
        let quad_pipeline = QuadPipeline::new(&device, &mut compiler, sample_count);
        let glyph_pipeline = GlyphPipeline::new(&device, &mut compiler, sample_count);
        let blit_pipeline =
            BlitPipeline::new(&device, &mut compiler, final_pass_target.resolve_view());

        let default_lightmap = create_texture(
            &device,
            &queue,
            None,
            1,
            1,
            &TextureData::Lightmap(LightmapData {
                lightmap: (&[0xFF][..]).into(),
            }),
        );
        let default_lightmap_view = default_lightmap.create_view(&Default::default());

        Ok(GraphicsState {
            device,
            queue,
            initial_pass_target,
            deferred_pass_target,
            final_pass_target,
            frame_uniform_buffer,
            entity_uniform_buffer,

            world_bind_group_layouts,
            world_bind_groups,

            sample_count: Cell::new(sample_count),

            alias_pipeline,
            brush_pipeline,
            sprite_pipeline,
            deferred_pipeline,
            particle_pipeline,
            postprocess_pipeline,
            glyph_pipeline,
            quad_pipeline,
            blit_pipeline,

            diffuse_sampler,
            lightmap_sampler,
            default_lightmap,
            default_lightmap_view,
            vfs,
            palette,
            gfx_wad,
            compiler: RefCell::new(compiler),
        })
    }

    pub fn create_texture<'a>(
        &self,
        label: Option<&'a str>,
        width: u32,
        height: u32,
        data: &TextureData,
    ) -> wgpu::Texture {
        create_texture(&self.device, &self.queue, label, width, height, data)
    }

    /// Update graphics state with the new framebuffer size and sample count.
    ///
    /// If the framebuffer size has changed, this recreates all render targets with the new size.
    ///
    /// If the framebuffer sample count has changed, this recreates all render targets with the
    /// new sample count and rebuilds the render pipelines to output that number of samples.
    pub fn update(&mut self, size: Extent2d, sample_count: u32) {
        if self.sample_count.get() != sample_count {
            self.sample_count.set(sample_count);
            self.recreate_pipelines(sample_count);
        }

        if self.initial_pass_target.size() != size
            || self.initial_pass_target.sample_count() != sample_count
        {
            self.initial_pass_target = InitialPassTarget::new(self.device(), size, sample_count);
        }

        if self.deferred_pass_target.size() != size
            || self.deferred_pass_target.sample_count() != sample_count
        {
            self.deferred_pass_target = DeferredPassTarget::new(self.device(), size, sample_count);
        }

        if self.final_pass_target.size() != size
            || self.final_pass_target.sample_count() != sample_count
        {
            self.final_pass_target = FinalPassTarget::new(self.device(), size, sample_count);
            self.blit_pipeline.rebuild(
                &self.device,
                &mut *self.compiler.borrow_mut(),
                self.final_pass_target.resolve_view(),
            )
        }
    }

    /// Rebuild all render pipelines using the new sample count.
    ///
    /// This must be called when the sample count of the render target(s) changes or the program
    /// will panic.
    fn recreate_pipelines(&mut self, sample_count: u32) {
        self.alias_pipeline.rebuild(
            &self.device,
            &mut self.compiler.borrow_mut(),
            &self.world_bind_group_layouts,
            sample_count,
        );
        self.brush_pipeline.rebuild(
            &self.device,
            &mut self.compiler.borrow_mut(),
            &self.world_bind_group_layouts,
            sample_count,
        );
        self.sprite_pipeline.rebuild(
            &self.device,
            &mut self.compiler.borrow_mut(),
            &self.world_bind_group_layouts,
            sample_count,
        );
        self.deferred_pipeline
            .rebuild(&self.device, &mut self.compiler.borrow_mut(), sample_count);
        self.postprocess_pipeline.rebuild(
            &self.device,
            &mut self.compiler.borrow_mut(),
            sample_count,
        );
        self.glyph_pipeline
            .rebuild(&self.device, &mut self.compiler.borrow_mut(), sample_count);
        self.quad_pipeline
            .rebuild(&self.device, &mut self.compiler.borrow_mut(), sample_count);
        self.blit_pipeline.rebuild(
            &self.device,
            &mut self.compiler.borrow_mut(),
            self.final_pass_target.resolve_view(),
        );
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn initial_pass_target(&self) -> &InitialPassTarget {
        &self.initial_pass_target
    }

    pub fn deferred_pass_target(&self) -> &DeferredPassTarget {
        &self.deferred_pass_target
    }

    pub fn final_pass_target(&self) -> &FinalPassTarget {
        &self.final_pass_target
    }

    pub fn frame_uniform_buffer(&self) -> &wgpu::Buffer {
        &self.frame_uniform_buffer
    }

    pub fn entity_uniform_buffer(&self) -> Ref<DynamicUniformBuffer<EntityUniforms>> {
        self.entity_uniform_buffer.borrow()
    }

    pub fn entity_uniform_buffer_mut(&self) -> RefMut<DynamicUniformBuffer<EntityUniforms>> {
        self.entity_uniform_buffer.borrow_mut()
    }

    pub fn diffuse_sampler(&self) -> &wgpu::Sampler {
        &self.diffuse_sampler
    }

    pub fn default_lightmap(&self) -> &wgpu::Texture {
        &self.default_lightmap
    }

    pub fn default_lightmap_view(&self) -> &wgpu::TextureView {
        &self.default_lightmap_view
    }

    pub fn lightmap_sampler(&self) -> &wgpu::Sampler {
        &self.lightmap_sampler
    }

    pub fn world_bind_group_layouts(&self) -> &[wgpu::BindGroupLayout] {
        &self.world_bind_group_layouts
    }

    pub fn world_bind_groups(&self) -> &[wgpu::BindGroup] {
        &self.world_bind_groups
    }

    // pipelines

    pub fn alias_pipeline(&self) -> &AliasPipeline {
        &self.alias_pipeline
    }

    pub fn brush_pipeline(&self) -> &BrushPipeline {
        &self.brush_pipeline
    }

    pub fn sprite_pipeline(&self) -> &SpritePipeline {
        &self.sprite_pipeline
    }

    pub fn deferred_pipeline(&self) -> &DeferredPipeline {
        &self.deferred_pipeline
    }

    pub fn particle_pipeline(&self) -> &ParticlePipeline {
        &self.particle_pipeline
    }

    pub fn postprocess_pipeline(&self) -> &PostProcessPipeline {
        &self.postprocess_pipeline
    }

    pub fn glyph_pipeline(&self) -> &GlyphPipeline {
        &self.glyph_pipeline
    }

    pub fn quad_pipeline(&self) -> &QuadPipeline {
        &self.quad_pipeline
    }

    pub fn blit_pipeline(&self) -> &BlitPipeline {
        &self.blit_pipeline
    }

    pub fn vfs(&self) -> &Vfs {
        &self.vfs
    }

    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    pub fn gfx_wad(&self) -> &Wad {
        &self.gfx_wad
    }
}

pub struct ClientRenderer {
    deferred_renderer: DeferredRenderer,
    postprocess_renderer: PostProcessRenderer,
    ui_renderer: UiRenderer,
    bump: Bump,
    start_time: DateTime<Utc>,
}

impl ClientRenderer {
    pub fn new(state: &GraphicsState, menu: &Menu) -> ClientRenderer {
        ClientRenderer {
            deferred_renderer: DeferredRenderer::new(
                state,
                state.initial_pass_target.diffuse_view(),
                state.initial_pass_target.normal_view(),
                state.initial_pass_target.light_view(),
                state.initial_pass_target.depth_view(),
            ),
            postprocess_renderer: PostProcessRenderer::new(
                state,
                state.deferred_pass_target.color_view(),
            ),
            ui_renderer: UiRenderer::new(state, menu),
            bump: Bump::new(),
            start_time: Utc::now(),
        }
    }

    pub fn render(
        &mut self,
        gfx_state: &GraphicsState,
        encoder: &mut wgpu::CommandEncoder,
        conn: Option<&Connection>,
        width: u32,
        height: u32,
        fov: Deg<f32>,
        cvars: &CvarRegistry,
        console: &Console,
        menu: &Menu,
        focus: InputFocus,
    ) {
        self.bump.reset();

        if let Some(Connection {
            state: ref cl_state,
            ref conn_state,
            ref kind,
        }) = conn
        {
            match conn_state {
                ConnectionState::Connected(ref world) => {
                    // if client is fully connected, draw world
                    let camera = match kind {
                        ConnectionKind::Demo(_) => {
                            cl_state.demo_camera(width as f32 / height as f32, fov)
                        }
                        ConnectionKind::Server { .. } => {
                            cl_state.camera(width as f32 / height as f32, fov)
                        }
                    };

                    // initial render pass
                    {
                        let init_pass_builder =
                            gfx_state.initial_pass_target().render_pass_builder();

                        let mut init_pass =
                            encoder.begin_render_pass(&init_pass_builder.descriptor());

                        world.render_pass(
                            gfx_state,
                            &mut init_pass,
                            &self.bump,
                            &camera,
                            cl_state.time(),
                            cl_state.iter_visible_entities(),
                            cl_state.iter_particles(),
                            cl_state.lightstyle_values().unwrap().as_slice(),
                            cl_state.viewmodel_id(),
                            cvars,
                        );
                    }

                    // deferred lighting pass
                    {
                        let deferred_pass_builder =
                            gfx_state.deferred_pass_target().render_pass_builder();
                        let mut deferred_pass =
                            encoder.begin_render_pass(&deferred_pass_builder.descriptor());

                        let mut lights = [PointLight {
                            origin: Vector3::zero(),
                            radius: 0.0,
                        }; MAX_LIGHTS];

                        let mut light_count = 0;
                        for (light_id, light) in cl_state.iter_lights().enumerate() {
                            light_count += 1;
                            let light_origin = light.origin();
                            let converted_origin =
                                Vector3::new(-light_origin.y, light_origin.z, -light_origin.x);
                            lights[light_id].origin =
                                (camera.view() * converted_origin.extend(1.0)).truncate();
                            lights[light_id].radius = light.radius(cl_state.time());
                        }

                        let uniforms = DeferredUniforms {
                            inv_projection: camera.inverse_projection().into(),
                            light_count,
                            _pad: [0; 3],
                            lights,
                        };

                        self.deferred_renderer.rebuild(
                            gfx_state,
                            gfx_state.initial_pass_target().diffuse_view(),
                            gfx_state.initial_pass_target().normal_view(),
                            gfx_state.initial_pass_target().light_view(),
                            gfx_state.initial_pass_target().depth_view(),
                        );

                        self.deferred_renderer
                            .record_draw(gfx_state, &mut deferred_pass, uniforms);
                    }
                }

                // if client is still signing on, draw the loading screen
                ConnectionState::SignOn(_) => {
                    // TODO: loading screen
                }
            }
        }

        let ui_state = match conn {
            Some(Connection {
                state: ref cl_state,
                ..
            }) => UiState::InGame {
                hud: match cl_state.intermission() {
                    Some(kind) => HudState::Intermission {
                        kind,
                        completion_duration: cl_state.completion_time().unwrap()
                            - cl_state.start_time(),
                        stats: cl_state.stats(),
                        console,
                    },

                    None => HudState::InGame {
                        items: cl_state.items(),
                        item_pickup_time: cl_state.item_pickup_times(),
                        stats: cl_state.stats(),
                        face_anim_time: cl_state.face_anim_time(),
                        console,
                    },
                },

                overlay: match focus {
                    InputFocus::Game => None,
                    InputFocus::Console => Some(UiOverlay::Console(console)),
                    InputFocus::Menu => Some(UiOverlay::Menu(menu)),
                },
            },

            None => UiState::Title {
                overlay: match focus {
                    InputFocus::Console => UiOverlay::Console(console),
                    InputFocus::Menu => UiOverlay::Menu(menu),
                    InputFocus::Game => unreachable!(),
                },
            },
        };

        // final render pass: postprocess the world and draw the UI
        {
            // quad_commands must outlive final pass
            let mut quad_commands = Vec::new();
            let mut glyph_commands = Vec::new();

            let final_pass_builder = gfx_state.final_pass_target().render_pass_builder();
            let mut final_pass = encoder.begin_render_pass(&final_pass_builder.descriptor());

            if let Some(Connection {
                state: ref cl_state,
                ref conn_state,
                ..
            }) = conn
            {
                // only postprocess if client is in the game
                if let ConnectionState::Connected(_) = conn_state {
                    self.postprocess_renderer
                        .rebuild(gfx_state, gfx_state.deferred_pass_target.color_view());
                    self.postprocess_renderer.record_draw(
                        gfx_state,
                        &mut final_pass,
                        cl_state.color_shift(),
                    );
                }
            }

            self.ui_renderer.render_pass(
                gfx_state,
                &mut final_pass,
                Extent2d { width, height },
                // use client time when in game, renderer time otherwise
                match conn {
                    Some(Connection { ref state, .. }) => state.time,
                    None => Utc::now().signed_duration_since(self.start_time),
                },
                &ui_state,
                &mut quad_commands,
                &mut glyph_commands,
            );
        }
    }
}
