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
/// - Final pass
///   - Inputs:
///     - `PostProcessPipeline`
///     - `QuadPipeline`
///     - `GlyphPipeline`
///   - Output: `FinalPassTarget`, which is resolved onto the framebuffer

// mod atlas;
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
pub use target::RenderTarget;
pub use ui::{hud::HudState, UiOverlay, UiRenderer, UiState};
pub use world::{Camera, WorldRenderer};

use std::{
    borrow::Cow,
    cell::{Cell, Ref, RefCell, RefMut},
    mem::size_of,
    rc::Rc,
};

use crate::{
    client::render::{
        target::{FinalPassTarget, InitialPassTarget},
        ui::{glyph, quad},
        uniform::DynamicUniformBuffer,
        world::{
            alias,
            brush::BrushPipeline,
            postprocess::{self, PostProcessPipeline},
            sprite, EntityUniforms,
        },
    },
    common::{util::any_slice_as_bytes, vfs::Vfs, wad::Wad},
};

use failure::Error;

const DEPTH_ATTACHMENT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub const DIFFUSE_ATTACHMENT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;
const NORMAL_ATTACHMENT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;

const DIFFUSE_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const FULLBRIGHT_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
const LIGHTMAP_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

/// Create a `wgpu::TextureDescriptor` appropriate for the provided texture data.
pub fn texture_descriptor<'a>(
    label: Option<&'a str>,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> wgpu::TextureDescriptor {
    wgpu::TextureDescriptor {
        label,
        size: wgpu::Extent3d {
            width,
            height,
            depth: 1,
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
        wgpu::TextureCopyView {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
        },
        data.data(),
        wgpu::TextureDataLayout {
            offset: 0,
            bytes_per_row: width * data.stride(),
            rows_per_image: 0,
        },
        wgpu::Extent3d {
            width,
            height,
            depth: 1,
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

impl std::convert::Into<wgpu::Extent3d> for Extent2d {
    fn into(self) -> wgpu::Extent3d {
        wgpu::Extent3d {
            width: self.width,
            height: self.height,
            depth: 1,
        }
    }
}

impl std::convert::From<winit::dpi::PhysicalSize<u32>> for Extent2d {
    fn from(other: winit::dpi::PhysicalSize<u32>) -> Extent2d {
        let winit::dpi::PhysicalSize { width, height } = other;
        Extent2d { width, height }
    }
}

/// Create a texture suitable for use as a color attachment.
///
/// This texture can be resolved using a swap chain texture as its target.
pub fn create_color_attachment(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    sample_count: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("color attachment"),
        size: wgpu::Extent3d {
            width,
            height,
            depth: 1,
        },
        mip_level_count: 1,
        sample_count,
        dimension: wgpu::TextureDimension::D2,
        format: DIFFUSE_ATTACHMENT_FORMAT,
        usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
    })
}

/// Create a texture suitable for use as a depth attachment.
pub fn create_depth_attachment(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    sample_count: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth attachment"),
        size: wgpu::Extent3d {
            width,
            height,
            depth: 1,
        },
        mip_level_count: 1,
        sample_count,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_ATTACHMENT_FORMAT,
        usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
    })
}

pub struct GraphicsState {
    device: wgpu::Device,
    queue: wgpu::Queue,
    initial_pass_target: InitialPassTarget,
    final_pass_target: FinalPassTarget,

    world_bind_group_layouts: Vec<wgpu::BindGroupLayout>,
    world_bind_groups: Vec<wgpu::BindGroup>,

    frame_uniform_buffer: wgpu::Buffer,

    entity_uniform_buffer: RefCell<DynamicUniformBuffer<EntityUniforms>>,
    diffuse_sampler: wgpu::Sampler,
    lightmap_sampler: wgpu::Sampler,

    sample_count: Cell<u32>,

    alias_pipeline: wgpu::RenderPipeline,
    alias_bind_group_layouts: Vec<wgpu::BindGroupLayout>,

    brush_pipeline: BrushPipeline,

    postprocess_pipeline: PostProcessPipeline,

    glyph_pipeline: wgpu::RenderPipeline,
    glyph_bind_group_layouts: Vec<wgpu::BindGroupLayout>,
    glyph_instance_buffer: wgpu::Buffer,

    quad_pipeline: wgpu::RenderPipeline,
    quad_bind_group_layouts: Vec<wgpu::BindGroupLayout>,
    quad_vertex_buffer: wgpu::Buffer,
    quad_uniform_buffer: RefCell<DynamicUniformBuffer<quad::QuadUniforms>>,

    sprite_pipeline: wgpu::RenderPipeline,
    sprite_bind_group_layouts: Vec<wgpu::BindGroupLayout>,
    sprite_vertex_buffer: wgpu::Buffer,

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
        let final_pass_target = FinalPassTarget::new(&device, size, sample_count);

        let frame_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame uniform buffer"),
            size: size_of::<world::FrameUniforms>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
            mapped_at_creation: false,
        });
        let entity_uniform_buffer = RefCell::new(DynamicUniformBuffer::new(&device));
        let quad_uniform_buffer = RefCell::new(DynamicUniformBuffer::new(&device));

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
            anisotropy_clamp: Some(16),
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
            anisotropy_clamp: Some(16),
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
                bindings: &[wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(frame_uniform_buffer.slice(..)),
                }],
            }),
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("brush per-entity bind group"),
                layout: &world_bind_group_layouts[world::BindGroupLayoutId::PerEntity as usize],
                bindings: &[
                    wgpu::Binding {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(
                            entity_uniform_buffer
                                .borrow()
                                .buffer()
                                .slice(0..entity_uniform_buffer.borrow().block_size().get()),
                        ),
                    },
                    wgpu::Binding {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&diffuse_sampler),
                    },
                    wgpu::Binding {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&lightmap_sampler),
                    },
                ],
            }),
        ];

        let (alias_pipeline, alias_bind_group_layouts) = alias::AliasPipeline::create(
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
        let (sprite_pipeline, sprite_bind_group_layouts) = sprite::SpritePipeline::create(
            &device,
            &mut compiler,
            &world_bind_group_layouts,
            sample_count,
        );
        let sprite_vertex_buffer = device.create_buffer_with_data(
            unsafe { any_slice_as_bytes(&sprite::VERTICES) },
            wgpu::BufferUsage::VERTEX,
        );

        let postprocess_pipeline = PostProcessPipeline::new(&device, &mut compiler, sample_count);

        let (quad_pipeline, quad_bind_group_layouts) =
            quad::QuadPipeline::create(&device, &mut compiler, &[], sample_count);
        let quad_vertex_buffer = device.create_buffer_with_data(
            unsafe { any_slice_as_bytes(&quad::VERTICES) },
            wgpu::BufferUsage::VERTEX,
        );

        let (glyph_pipeline, glyph_bind_group_layouts) =
            glyph::GlyphPipeline::create(&device, &mut compiler, &[], sample_count);
        let glyph_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("quad instance buffer"),
            size: (glyph::GLYPH_MAX_INSTANCES * size_of::<glyph::GlyphInstance>()) as u64,
            usage: wgpu::BufferUsage::VERTEX | wgpu::BufferUsage::COPY_DST,
            mapped_at_creation: false,
        });

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
        let default_lightmap_view = default_lightmap.create_default_view();

        Ok(GraphicsState {
            device,
            queue,
            initial_pass_target,
            final_pass_target,
            frame_uniform_buffer,
            entity_uniform_buffer,

            world_bind_group_layouts,
            world_bind_groups,

            sample_count: Cell::new(sample_count),

            alias_pipeline,
            alias_bind_group_layouts,
            brush_pipeline,
            sprite_pipeline,
            sprite_bind_group_layouts,
            sprite_vertex_buffer,
            postprocess_pipeline,
            glyph_pipeline,
            glyph_bind_group_layouts,
            glyph_instance_buffer,
            quad_pipeline,
            quad_bind_group_layouts,
            quad_vertex_buffer,
            quad_uniform_buffer,
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

        if self.final_pass_target.size() != size
            || self.final_pass_target.sample_count() != sample_count
        {
            self.final_pass_target = FinalPassTarget::new(self.device(), size, sample_count);
        }
    }

    /// Rebuild all render pipelines using the new sample count.
    ///
    /// This must be called when the sample count of the render target(s) changes or the program
    /// will panic.
    fn recreate_pipelines(&mut self, sample_count: u32) {
        let world_bind_group_layouts: Vec<_> = self.world_bind_group_layouts.iter().collect();

        let mut alias_bind_group_layouts = world_bind_group_layouts.clone();
        alias_bind_group_layouts.extend(self.alias_bind_group_layouts.iter());
        self.alias_pipeline = alias::AliasPipeline::recreate(
            &self.device,
            &mut self.compiler.borrow_mut(),
            &alias_bind_group_layouts,
            sample_count,
        );

        self.brush_pipeline.rebuild(
            &self.device,
            &mut self.compiler.borrow_mut(),
            &self.world_bind_group_layouts,
            sample_count,
        );

        let mut sprite_bind_group_layouts = world_bind_group_layouts.clone();
        sprite_bind_group_layouts.extend(self.sprite_bind_group_layouts.iter());
        self.sprite_pipeline = sprite::SpritePipeline::recreate(
            &self.device,
            &mut self.compiler.borrow_mut(),
            &sprite_bind_group_layouts,
            sample_count,
        );

        self.postprocess_pipeline.rebuild(
            &self.device,
            &mut self.compiler.borrow_mut(),
            sample_count,
        );

        let glyph_bind_group_layouts: Vec<_> = self.glyph_bind_group_layouts.iter().collect();
        self.glyph_pipeline = glyph::GlyphPipeline::recreate(
            &self.device,
            &mut self.compiler.borrow_mut(),
            &glyph_bind_group_layouts,
            sample_count,
        );

        let quad_bind_group_layouts: Vec<_> = self.quad_bind_group_layouts.iter().collect();
        self.quad_pipeline = quad::QuadPipeline::recreate(
            &self.device,
            &mut self.compiler.borrow_mut(),
            &quad_bind_group_layouts,
            sample_count,
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

    pub fn alias_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.alias_pipeline
    }

    pub fn alias_bind_group_layout(&self, id: world::BindGroupLayoutId) -> &wgpu::BindGroupLayout {
        &self.alias_bind_group_layouts[id as usize - 2]
    }

    pub fn brush_pipeline(&self) -> &BrushPipeline {
        &self.brush_pipeline
    }

    pub fn postprocess_pipeline(&self) -> &PostProcessPipeline {
        &self.postprocess_pipeline
    }

    // glyph pipeline

    pub fn glyph_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.glyph_pipeline
    }

    pub fn glyph_bind_group_layouts(&self) -> &[wgpu::BindGroupLayout] {
        &self.glyph_bind_group_layouts
    }

    pub fn glyph_instance_buffer(&self) -> &wgpu::Buffer {
        &self.glyph_instance_buffer
    }

    // quad pipeline(s)

    pub fn quad_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.quad_pipeline
    }

    pub fn quad_bind_group_layouts(&self) -> &[wgpu::BindGroupLayout] {
        &self.quad_bind_group_layouts
    }

    pub fn quad_vertex_buffer(&self) -> &wgpu::Buffer {
        &self.quad_vertex_buffer
    }

    pub fn quad_uniform_buffer(&self) -> Ref<DynamicUniformBuffer<ui::quad::QuadUniforms>> {
        self.quad_uniform_buffer.borrow()
    }

    pub fn quad_uniform_buffer_mut(&self) -> RefMut<DynamicUniformBuffer<ui::quad::QuadUniforms>> {
        self.quad_uniform_buffer.borrow_mut()
    }

    // sprite pipeline

    pub fn sprite_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.sprite_pipeline
    }

    pub fn sprite_bind_group_layout(&self, id: world::BindGroupLayoutId) -> &wgpu::BindGroupLayout {
        &self.sprite_bind_group_layouts[id as usize - 2]
    }

    pub fn sprite_bind_group_layouts(&self) -> &[wgpu::BindGroupLayout] {
        &self.sprite_bind_group_layouts
    }

    pub fn sprite_vertex_buffer(&self) -> &wgpu::Buffer {
        &self.sprite_vertex_buffer
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
