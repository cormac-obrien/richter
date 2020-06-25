// mod atlas;
mod error;
mod palette;
mod ui;
mod uniform;
mod warp;
mod world;

pub use error::{RenderError, RenderErrorKind};
pub use palette::Palette;
pub use ui::{hud::HudState, UiOverlay, UiRenderer, UiState};
pub use world::{Camera, WorldRenderer};

use std::{
    borrow::Cow,
    cell::{Ref, RefCell, RefMut},
    mem::size_of,
    rc::Rc,
};

use crate::{
    client::render::wgpu::{
        ui::{glyph, quad},
        uniform::{DynamicUniformBuffer, DynamicUniformBufferBlock},
        world::{alias, brush, sprite, EntityUniforms},
    },
    common::{util::any_slice_as_bytes, vfs::Vfs, wad::Wad},
};

use cgmath::{Deg, Euler, Matrix4};
use failure::{Error, Fail};
use shaderc::{CompileOptions, Compiler};
use strum::IntoEnumIterator;

pub const COLOR_ATTACHMENT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;
const DEPTH_ATTACHMENT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const DIFFUSE_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const FULLBRIGHT_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
const LIGHTMAP_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

pub trait Pipeline {
    fn name() -> &'static str;
    fn bind_group_layout_descriptors() -> Vec<wgpu::BindGroupLayoutDescriptor<'static>>;
    fn vertex_shader() -> &'static str;
    fn fragment_shader() -> &'static str;
    fn rasterization_state_descriptor() -> Option<wgpu::RasterizationStateDescriptor>;
    fn primitive_topology() -> wgpu::PrimitiveTopology;
    fn color_state_descriptors() -> Vec<wgpu::ColorStateDescriptor>;
    fn depth_stencil_state_descriptor() -> Option<wgpu::DepthStencilStateDescriptor>;
    fn vertex_buffer_descriptors() -> Vec<wgpu::VertexBufferDescriptor<'static>>;
}

// bind_group_layout_prefix is a set of bind group layouts to be prefixed onto
// P::BIND_GROUP_LAYOUT_DESCRIPTORS in order to allow layout reuse between pipelines
pub fn create_pipeline<'a, P>(
    device: &wgpu::Device,
    compiler: &mut shaderc::Compiler,
    bind_group_layout_prefix: &[wgpu::BindGroupLayout],
) -> (wgpu::RenderPipeline, Vec<wgpu::BindGroupLayout>)
where
    P: Pipeline,
{
    info!("Creating {} pipeline", P::name());
    let bind_group_layouts = P::bind_group_layout_descriptors()
        .iter()
        .map(|desc| device.create_bind_group_layout(desc))
        .collect::<Vec<_>>();
    info!(
        "{} layouts in prefix | {} specific to pipeline",
        bind_group_layout_prefix.len(),
        bind_group_layouts.len(),
    );

    let pipeline_layout = {
        // add bind group layout prefix
        let layouts: Vec<&wgpu::BindGroupLayout> = bind_group_layout_prefix
            .iter()
            .chain(bind_group_layouts.iter())
            .collect();
        info!("{} layouts total", layouts.len());
        let desc = wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &layouts,
        };
        device.create_pipeline_layout(&desc)
    };

    let vertex_shader_spirv = compiler
        .compile_into_spirv(
            P::vertex_shader().as_ref(),
            shaderc::ShaderKind::Vertex,
            &format!("{}.vert", P::name()),
            "main",
            None,
        )
        .unwrap();
    let vertex_shader = device.create_shader_module(wgpu::ShaderModuleSource::SpirV(
        vertex_shader_spirv.as_binary(),
    ));
    let fragment_shader_spirv = compiler
        .compile_into_spirv(
            P::fragment_shader().as_ref(),
            shaderc::ShaderKind::Fragment,
            &format!("{}.frag", P::name()),
            "main",
            None,
        )
        .unwrap();
    let fragment_shader = device.create_shader_module(wgpu::ShaderModuleSource::SpirV(
        fragment_shader_spirv.as_binary(),
    ));

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        layout: &pipeline_layout,
        vertex_stage: wgpu::ProgrammableStageDescriptor {
            module: &vertex_shader,
            entry_point: "main",
        },
        fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
            module: &fragment_shader,
            entry_point: "main",
        }),
        rasterization_state: P::rasterization_state_descriptor(),
        primitive_topology: P::primitive_topology(),
        color_states: &P::color_state_descriptors(),
        depth_stencil_state: P::depth_stencil_state_descriptor(),
        vertex_state: wgpu::VertexStateDescriptor {
            index_format: wgpu::IndexFormat::Uint32,
            vertex_buffers: &P::vertex_buffer_descriptors(),
        },
        sample_count: 1,
        sample_mask: !0,
        alpha_to_coverage_enabled: false,
    });

    (pipeline, bind_group_layouts)
}

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

pub struct GraphicsState<'a> {
    device: wgpu::Device,
    queue: wgpu::Queue,
    depth_attachment: RefCell<wgpu::Texture>,

    world_bind_group_layouts: Vec<wgpu::BindGroupLayout>,
    world_bind_groups: Vec<wgpu::BindGroup>,

    frame_uniform_buffer: wgpu::Buffer,

    entity_uniform_buffer: RefCell<DynamicUniformBuffer<'a, EntityUniforms>>,
    diffuse_sampler: wgpu::Sampler,
    lightmap_sampler: wgpu::Sampler,

    alias_pipeline: wgpu::RenderPipeline,
    alias_bind_group_layouts: Vec<wgpu::BindGroupLayout>,

    brush_pipeline: wgpu::RenderPipeline,
    brush_bind_group_layouts: Vec<wgpu::BindGroupLayout>,
    brush_texture_uniform_buffer: RefCell<DynamicUniformBuffer<'a, brush::TextureUniforms>>,
    brush_texture_uniform_blocks: Vec<DynamicUniformBufferBlock<'a, brush::TextureUniforms>>,

    glyph_pipeline: wgpu::RenderPipeline,
    glyph_bind_group_layouts: Vec<wgpu::BindGroupLayout>,
    glyph_instance_buffer: wgpu::Buffer,

    quad_pipeline: wgpu::RenderPipeline,
    quad_bind_group_layouts: Vec<wgpu::BindGroupLayout>,
    quad_vertex_buffer: wgpu::Buffer,
    quad_uniform_buffer: RefCell<DynamicUniformBuffer<'a, quad::QuadUniforms>>,

    sprite_pipeline: wgpu::RenderPipeline,
    sprite_bind_group_layouts: Vec<wgpu::BindGroupLayout>,
    sprite_vertex_buffer: wgpu::Buffer,

    default_diffuse: wgpu::Texture,
    default_diffuse_view: wgpu::TextureView,
    default_fullbright: wgpu::Texture,
    default_fullbright_view: wgpu::TextureView,
    default_lightmap: wgpu::Texture,
    default_lightmap_view: wgpu::TextureView,

    vfs: Rc<Vfs>,
    palette: Palette,
    gfx_wad: Wad,
}

impl<'a> GraphicsState<'a> {
    pub fn new<'b>(
        device: wgpu::Device,
        queue: wgpu::Queue,
        width: u32,
        height: u32,
        vfs: Rc<Vfs>,
    ) -> Result<GraphicsState<'a>, Error> {
        let palette = Palette::load(&vfs, "gfx/palette.lmp");
        let gfx_wad = Wad::load(vfs.open("gfx.wad")?).unwrap();
        let mut compiler = shaderc::Compiler::new().unwrap();

        let depth_attachment = RefCell::new(device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth attachment"),
            size: wgpu::Extent3d {
                width,
                height,
                depth: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_ATTACHMENT_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
        }));

        let frame_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame uniform buffer"),
            size: size_of::<world::FrameUniforms>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
            mapped_at_creation: false,
        });
        let entity_uniform_buffer = RefCell::new(DynamicUniformBuffer::new(&device));
        let brush_texture_uniform_buffer = RefCell::new(DynamicUniformBuffer::new(&device));
        let brush_texture_uniform_blocks = brush::TextureKind::iter()
            .map(|kind| {
                debug!("Texture kind: {:?} ({})", kind, kind as u32);
                brush_texture_uniform_buffer
                    .borrow_mut()
                    .allocate(brush::TextureUniforms { kind })
            })
            .collect();
        brush_texture_uniform_buffer.borrow_mut().flush(&queue);
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
            anisotropy_clamp: None,
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
            anisotropy_clamp: None,
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

        let (alias_pipeline, alias_bind_group_layouts) = create_pipeline::<alias::AliasPipeline>(
            &device,
            &mut compiler,
            &world_bind_group_layouts,
        );
        let (brush_pipeline, brush_bind_group_layouts) = create_pipeline::<brush::BrushPipeline>(
            &device,
            &mut compiler,
            &world_bind_group_layouts,
        );
        let (sprite_pipeline, sprite_bind_group_layouts) = create_pipeline::<sprite::SpritePipeline>(
            &device,
            &mut compiler,
            &world_bind_group_layouts,
        );
        let sprite_vertex_buffer = device.create_buffer_with_data(
            unsafe { any_slice_as_bytes(&sprite::VERTICES) },
            wgpu::BufferUsage::VERTEX,
        );

        let (quad_pipeline, quad_bind_group_layouts) =
            create_pipeline::<quad::QuadPipeline>(&device, &mut compiler, &[]);
        let quad_vertex_buffer = device.create_buffer_with_data(
            unsafe { any_slice_as_bytes(&quad::VERTICES) },
            wgpu::BufferUsage::VERTEX,
        );

        let (glyph_pipeline, glyph_bind_group_layouts) =
            create_pipeline::<glyph::GlyphPipeline>(&device, &mut compiler, &[]);
        let glyph_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("quad instance buffer"),
            size: (glyph::GLYPH_MAX_INSTANCES * size_of::<glyph::GlyphInstance>()) as u64,
            usage: wgpu::BufferUsage::VERTEX | wgpu::BufferUsage::COPY_DST,
            mapped_at_creation: false,
        });

        let default_diffuse = create_texture(
            &device,
            &queue,
            None,
            2,
            2,
            &TextureData::Diffuse(DiffuseData {
                // taking a page out of Valve's book with the pink-and-black checkerboard
                rgba: (&[
                    0xFF, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x00, 0xFF, 0xFF,
                    0x00, 0xFF, 0xFF,
                ][..])
                    .into(),
            }),
        );
        let default_fullbright = create_texture(
            &device,
            &queue,
            None,
            1,
            1,
            &TextureData::Fullbright(FullbrightData {
                fullbright: (&[0xFF][..]).into(),
            }),
        );
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

        let default_diffuse_view = default_diffuse.create_default_view();
        let default_fullbright_view = default_fullbright.create_default_view();
        let default_lightmap_view = default_lightmap.create_default_view();

        Ok(GraphicsState {
            device,
            queue,
            depth_attachment,
            frame_uniform_buffer,
            entity_uniform_buffer,

            world_bind_group_layouts,
            world_bind_groups,

            alias_pipeline,
            alias_bind_group_layouts,
            brush_pipeline,
            brush_bind_group_layouts,
            brush_texture_uniform_buffer,
            brush_texture_uniform_blocks,
            glyph_pipeline,
            glyph_bind_group_layouts,
            glyph_instance_buffer,
            quad_pipeline,
            quad_bind_group_layouts,
            quad_vertex_buffer,
            quad_uniform_buffer,
            sprite_pipeline,
            sprite_bind_group_layouts,
            sprite_vertex_buffer,
            diffuse_sampler,
            lightmap_sampler,
            default_diffuse,
            default_diffuse_view,
            default_fullbright,
            default_fullbright_view,
            default_lightmap,
            default_lightmap_view,
            vfs,
            palette,
            gfx_wad,
        })
    }

    pub fn create_texture<'b>(
        &self,
        label: Option<&'b str>,
        width: u32,
        height: u32,
        data: &TextureData,
    ) -> wgpu::Texture {
        create_texture(&self.device, &self.queue, label, width, height, data)
    }

    /// Creates a new depth attachment with the specified dimensions, replacing the old one.
    pub fn recreate_depth_attachment(&self, width: u32, height: u32) {
        let depth_attachment = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth attachment"),
            size: wgpu::Extent3d {
                width,
                height,
                depth: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_ATTACHMENT_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
        });
        let _ = self.depth_attachment.replace(depth_attachment);
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn depth_attachment(&self) -> Ref<wgpu::Texture> {
        self.depth_attachment.borrow()
    }

    pub fn frame_uniform_buffer(&self) -> &wgpu::Buffer {
        &self.frame_uniform_buffer
    }

    pub fn entity_uniform_buffer(&self) -> Ref<DynamicUniformBuffer<'a, EntityUniforms>> {
        self.entity_uniform_buffer.borrow()
    }

    pub fn entity_uniform_buffer_mut(&self) -> RefMut<DynamicUniformBuffer<'a, EntityUniforms>> {
        self.entity_uniform_buffer.borrow_mut()
    }

    pub fn diffuse_sampler(&self) -> &wgpu::Sampler {
        &self.diffuse_sampler
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

    // brush pipeline

    pub fn brush_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.brush_pipeline
    }

    pub fn brush_bind_group_layout(&self, id: world::BindGroupLayoutId) -> &wgpu::BindGroupLayout {
        &self.brush_bind_group_layouts[id as usize - 2]
    }

    pub fn brush_bind_group_layouts(&self) -> &[wgpu::BindGroupLayout] {
        &self.brush_bind_group_layouts
    }

    pub fn brush_texture_uniform_buffer(
        &self,
    ) -> Ref<DynamicUniformBuffer<'a, brush::TextureUniforms>> {
        self.brush_texture_uniform_buffer.borrow()
    }

    pub fn brush_texture_uniform_buffer_mut(
        &self,
    ) -> RefMut<DynamicUniformBuffer<'a, brush::TextureUniforms>> {
        self.brush_texture_uniform_buffer.borrow_mut()
    }

    pub fn brush_texture_uniform_block(
        &self,
        kind: brush::TextureKind,
    ) -> &DynamicUniformBufferBlock<'a, brush::TextureUniforms> {
        &self.brush_texture_uniform_blocks[kind as usize]
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

    pub fn quad_uniform_buffer(&self) -> Ref<DynamicUniformBuffer<'a, ui::quad::QuadUniforms>> {
        self.quad_uniform_buffer.borrow()
    }

    pub fn quad_uniform_buffer_mut(
        &self,
    ) -> RefMut<DynamicUniformBuffer<'a, ui::quad::QuadUniforms>> {
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
