mod atlas;
mod brush;
mod error;
mod palette;

pub use error::{RenderError, RenderErrorKind};
pub use palette::Palette;

use std::{
    borrow::Cow,
    cell::{Cell, Ref, RefCell},
    marker::PhantomData,
    mem::size_of,
    rc::Rc,
};

use crate::{
    client::{
        render::wgpu::brush::{BrushRenderer, BrushRendererBuilder},
        ClientEntity,
    },
    common::{
        engine,
        model::{Model, ModelKind},
        util::{any_as_bytes, bytes_as_any},
        vfs::Vfs,
        wad::{QPic, Wad},
    },
};

use cgmath::{Deg, Euler, Matrix4, Vector3, Vector4, Zero};
use chrono::Duration;
use failure::{Error, Fail};
use shaderc::{CompileOptions, Compiler};

pub const COLOR_ATTACHMENT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;
const DEPTH_ATTACHMENT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const DIFFUSE_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const FULLBRIGHT_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
const LIGHTMAP_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

const PER_FRAME_BIND_GROUP_LAYOUT_DESCRIPTOR: wgpu::BindGroupLayoutDescriptor =
    wgpu::BindGroupLayoutDescriptor {
        label: Some("per-frame bind group"),
        bindings: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStage::all(),
            ty: wgpu::BindingType::UniformBuffer { dynamic: false },
        }],
    };

pub fn calculate_transform(
    camera: &Camera,
    origin: Vector3<f32>,
    angles: Vector3<Deg<f32>>,
) -> Matrix4<f32> {
    camera.transform()
        * Matrix4::from_translation(Vector3::new(-origin.y, origin.z, -origin.x))
        * Matrix4::from(Euler::new(angles.x, angles.y, angles.z))
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
        array_layer_count: 1,
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
    let staging_buffer = device.create_buffer_with_data(data.data(), wgpu::BufferUsage::COPY_SRC);
    let texture = device.create_texture(&texture_descriptor(label, width, height, data.format()));
    let mut command_encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    command_encoder.copy_buffer_to_texture(
        wgpu::BufferCopyView {
            buffer: &staging_buffer,
            offset: 0,
            bytes_per_row: width * data.stride(),
            rows_per_image: height,
        },
        wgpu::TextureCopyView {
            texture: &texture,
            mip_level: 0,
            array_layer: 0,
            origin: wgpu::Origin3d::ZERO,
        },
        wgpu::Extent3d {
            width,
            height,
            depth: 1,
        },
    );
    let command_buffer = command_encoder.finish();
    queue.submit(&[command_buffer]);

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
            TextureData::Diffuse(_) => size_of::<[f32; 4]>(),
            TextureData::Fullbright(_) => size_of::<f32>(),
            TextureData::Lightmap(_) => size_of::<f32>(),
        }) as u32
    }

    pub fn size(&self) -> wgpu::BufferAddress {
        self.data().len() as wgpu::BufferAddress
    }
}

pub struct Camera {
    origin: Vector3<f32>,
    transform: Matrix4<f32>,
}

impl Camera {
    pub fn new(
        origin: Vector3<f32>,
        angles: Vector3<Deg<f32>>,
        projection: Matrix4<f32>,
    ) -> Camera {
        // convert coordinates
        let converted_origin = Vector3::new(-origin.y, origin.z, -origin.x);
        // translate the world by inverse of camera position
        let translation = Matrix4::from_translation(-converted_origin);
        let rotation = Matrix4::from(Euler::new(angles.x, -angles.y, -angles.z));

        Camera {
            origin,
            transform: projection * rotation * translation,
        }
    }

    pub fn origin(&self) -> Vector3<f32> {
        self.origin
    }

    pub fn transform(&self) -> Matrix4<f32> {
        self.transform
    }
}

// this is the minimum required maximum size of a uniform buffer in Vulkan, see
// https://www.khronos.org/registry/vulkan/specs/1.2-extensions/html/vkspec.html#limits-maxUniformBufferRange
const DYNAMIC_UNIFORM_BUFFER_SIZE: wgpu::BufferAddress = 16384;

/// A handle to a dynamic uniform buffer on the GPU.
///
/// Allows allocation and updating of individual blocks of memory.
pub struct DynamicUniformBuffer<'a, T>
where
    T: 'static + Copy + Sized + Send + Sync,
{
    // keeps track of how many blocks are allocated so we know whether we can
    // clear the buffer or not
    _rc: RefCell<Rc<()>>,

    // represents the data in the buffer, which we don't actually own
    _phantom: PhantomData<&'a [T]>,

    inner: wgpu::Buffer,
    allocated: Cell<wgpu::BufferAddress>,
    mapped: RefCell<Option<wgpu::BufferWriteMapping>>,
}

impl<'a, T> DynamicUniformBuffer<'a, T>
where
    T: 'static + Copy + Sized + Send + Sync,
{
    pub fn new<'b>(device: &'b wgpu::Device) -> DynamicUniformBuffer<'a, T> {
        let inner = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dynamic uniform buffer"),
            size: DYNAMIC_UNIFORM_BUFFER_SIZE,
            usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::MAP_WRITE,
        });

        DynamicUniformBuffer {
            _rc: RefCell::new(Rc::new(())),
            _phantom: PhantomData,
            inner,
            allocated: Cell::new(0),
            mapped: RefCell::new(None),
        }
    }

    /// Allocates a block of memory in this dynamic uniform buffer.
    pub fn allocate(&self) -> DynamicUniformBufferBlock<'a, T> {
        trace!("Allocating dynamic uniform block");
        let allocated = self.allocated.get();
        let size = size_of::<T>() as wgpu::BufferAddress;
        if allocated + size > DYNAMIC_UNIFORM_BUFFER_SIZE {
            panic!(
                "Not enough space to allocate {} bytes in dynamic uniform buffer",
                size
            );
        }

        let addr = allocated;
        self.allocated.set(allocated + size);

        DynamicUniformBufferBlock {
            _rc: self._rc.borrow().clone(),
            _phantom: PhantomData,
            addr,
        }
    }

    /// Maps the underlying buffer into host memory for writing.
    ///
    /// Once the mapping is available, individual blocks can be updated with
    /// `update()`.
    pub async fn map_write(&self, device: &wgpu::Device) {
        if self.mapped.borrow().is_some() {
            panic!("Can't map buffer: already mapped");
        }

        let future = self.inner.map_write(0, self.allocated.get());

        // block until mapped
        device.poll(wgpu::Maintain::Wait);

        match future.await {
            Ok(mapped) => {
                self.mapped.replace(Some(mapped));
            }
            Err(e) => panic!("Can't map buffer: {:?}", e),
        }
    }

    /// Indicates whether the underlying buffer is mapped in host memory.
    pub fn mapped(&self) -> bool {
        self.mapped.borrow().is_some()
    }

    /// Reads the value stored in the given buffer block.
    ///
    /// Panics if the buffer is not mapped.
    pub fn value(&self, block: &DynamicUniformBufferBlock<'a, T>) -> T {
        if let Some(ref mut mapped) = *self.mapped.borrow_mut() {
            let addr = block.addr as usize;
            unsafe { bytes_as_any(&mapped.as_slice()[addr..addr + size_of::<T>()]) }
        } else {
            panic!("Can't read block value: dynamic uniform buffer is not mapped");
        }
    }

    /// Updates a block of buffer memory with the given data.
    ///
    /// Panics if the provided data is of the wrong size or if the buffer is not mapped.
    pub fn update(&self, block: &DynamicUniformBufferBlock<'a, T>, val: T) {
        let val_as_bytes = unsafe { any_as_bytes(&val) };

        if let Some(ref mut mapped) = *self.mapped.borrow_mut() {
            let addr = block.addr as usize;
            (&mut mapped.as_slice()[addr..addr + size_of::<T>()]).copy_from_slice(val_as_bytes);
        } else {
            panic!("Can't update block: dynamic uniform buffer is not mapped");
        }
    }

    /// Unmaps the underlying buffer from host memory, flushing any pending write operations.
    pub fn unmap(&self) {
        if !self.mapped() {
            panic!("Can't unmap buffer: buffer not mapped");
        }

        self.mapped.replace(None);
        self.inner.unmap();
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        // TODO: is this check necessary?
        if self.mapped() {
            panic!("Can't get inner buffer: there may be pending writes. Unmap the buffer first.");
        }

        &self.inner
    }

    /// Removes all allocations from the underlying buffer.
    ///
    /// Returns an error if the buffer is currently mapped or there are
    /// outstanding allocated blocks.
    pub fn clear(&self) -> Result<(), Error> {
        if self.mapped() {
            panic!(
                "Can't clear uniform buffer: there may be pending writes. Unmap the buffer first."
            );
        }

        let mut out = self._rc.replace(Rc::new(()));
        match Rc::try_unwrap(out) {
            // no outstanding blocks
            Ok(()) => {
                self.allocated.set(0);
                Ok(())
            }
            Err(rc) => {
                let _ = self._rc.replace(rc);
                bail!("Can't clear uniform buffer: there are outstanding references to allocated blocks.");
            }
        }
    }
}

/// An address into a dynamic uniform buffer.
#[derive(Debug)]
pub struct DynamicUniformBufferBlock<'a, T> {
    _rc: Rc<()>,
    _phantom: PhantomData<&'a T>,

    addr: wgpu::BufferAddress,
}

impl<'a, T> DynamicUniformBufferBlock<'a, T> {
    pub fn offset(&self) -> wgpu::DynamicOffset {
        self.addr as wgpu::DynamicOffset
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
// TODO: derive Debug once const generics are stable
pub struct FrameUniforms {
    lightmap_anim_frames: [f32; 64],
    time: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct EntityUniforms {
    transform: Matrix4<f32>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LightstyleUniforms {
    lightstyle_value: f32,
}

pub struct GraphicsPackage<'a> {
    device: wgpu::Device,
    queue: wgpu::Queue,
    depth_attachment: RefCell<wgpu::Texture>,
    frame_uniform_buffer: wgpu::Buffer,
    entity_uniform_buffer: DynamicUniformBuffer<'a, EntityUniforms>,
    brush_pipeline: wgpu::RenderPipeline,
    brush_bind_group_layouts: Vec<wgpu::BindGroupLayout>,
    diffuse_sampler: wgpu::Sampler,
    lightmap_sampler: wgpu::Sampler,

    default_diffuse: wgpu::Texture,
    default_diffuse_view: wgpu::TextureView,
    default_fullbright: wgpu::Texture,
    default_fullbright_view: wgpu::TextureView,
    default_lightmap: wgpu::Texture,
    default_lightmap_view: wgpu::TextureView,

    palette: Palette,
}

impl<'a> GraphicsPackage<'a> {
    pub fn new<'b>(
        device: wgpu::Device,
        queue: wgpu::Queue,
        width: u32,
        height: u32,
        vfs: &'b Vfs,
    ) -> Result<GraphicsPackage<'a>, Error> {
        let palette = Palette::load(&vfs, "gfx/palette.lmp");
        let gfx_wad = Wad::load(vfs.open("gfx.wad")?).unwrap();

        let depth_attachment = RefCell::new(device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth attachment"),
            size: wgpu::Extent3d {
                width,
                height,
                depth: 1,
            },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_ATTACHMENT_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
        }));

        let frame_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame uniform buffer"),
            size: size_of::<FrameUniforms>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::MAP_WRITE,
        });
        let entity_uniform_buffer = DynamicUniformBuffer::new(&device);

        let (brush_pipeline, brush_bind_group_layouts) = brush::create_render_pipeline(&device);

        let diffuse_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            // TODO: these are the OpenGL defaults; see if there's a better choice for us
            lod_min_clamp: -1000.0,
            lod_max_clamp: 1000.0,
            compare: wgpu::CompareFunction::Undefined,
        });

        let lightmap_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            // TODO: these are the OpenGL defaults; see if there's a better choice for us
            lod_min_clamp: -1000.0,
            lod_max_clamp: 1000.0,
            compare: wgpu::CompareFunction::Undefined,
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

        Ok(GraphicsPackage {
            device,
            queue,
            depth_attachment,
            frame_uniform_buffer,
            entity_uniform_buffer,
            brush_pipeline,
            brush_bind_group_layouts,
            diffuse_sampler,
            lightmap_sampler,
            default_diffuse,
            default_diffuse_view,
            default_fullbright,
            default_fullbright_view,
            default_lightmap,
            default_lightmap_view,
            palette,
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

    pub fn create_vertex_buffer<'b, V>(
        &self,
        label: Option<&'b str>,
        vertices: &[V],
    ) -> wgpu::Buffer {
        let size = vertices.len() * size_of::<V>();
        let bytes = unsafe { std::slice::from_raw_parts(vertices.as_ptr() as *const u8, size) };

        let staging_buffer = self
            .device
            .create_buffer_with_data(bytes, wgpu::BufferUsage::COPY_SRC);
        let vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label,
            size: size as wgpu::BufferAddress,
            usage: wgpu::BufferUsage::COPY_DST | wgpu::BufferUsage::VERTEX,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_buffer_to_buffer(
            &staging_buffer,
            0,
            &vertex_buffer,
            0,
            size as wgpu::BufferAddress,
        );
        let cmd_buffer = encoder.finish();
        self.queue.submit(&[cmd_buffer]);

        vertex_buffer
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
            array_layer_count: 1,
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

    pub fn depth_attachment(&self) -> Ref<wgpu::Texture> {
        self.depth_attachment.borrow()
    }

    pub fn frame_uniform_buffer(&self) -> &wgpu::Buffer {
        &self.frame_uniform_buffer
    }

    pub fn entity_uniform_buffer(&self) -> &DynamicUniformBuffer<'a, EntityUniforms> {
        &self.entity_uniform_buffer
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

    pub fn brush_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.brush_pipeline
    }

    pub fn brush_bind_group_layout(&self, id: brush::BindGroupLayoutId) -> &wgpu::BindGroupLayout {
        &self.brush_bind_group_layouts[id as usize]
    }

    pub fn brush_bind_group_layouts(&self) -> &[wgpu::BindGroupLayout] {
        &self.brush_bind_group_layouts
    }

    pub fn palette(&self) -> &Palette {
        &self.palette
    }
}

enum EntityRenderer<'a> {
    Brush(BrushRenderer<'a>),
}

/// Top-level renderer.
pub struct Renderer<'a> {
    gfx_pkg: Rc<GraphicsPackage<'a>>,

    // TODO: make this a proper WorldRenderer with visibility rendering
    world_renderer: BrushRenderer<'a>,
    entity_renderers: Vec<EntityRenderer<'a>>,

    world_uniform_block: DynamicUniformBufferBlock<'a, EntityUniforms>,
    entity_uniform_blocks: RefCell<Vec<DynamicUniformBufferBlock<'a, EntityUniforms>>>,

    per_frame_bind_group_layout: wgpu::BindGroupLayout,
    per_frame_bind_group: wgpu::BindGroup,
}

impl<'a> Renderer<'a> {
    pub fn new(
        models: &[Model],
        worldmodel_id: usize,
        gfx_pkg: Rc<GraphicsPackage<'a>>,
    ) -> Renderer<'a> {
        let mut world_renderer = None;
        let mut entity_renderers = Vec::new();
        let world_uniform_block = gfx_pkg.entity_uniform_buffer().allocate();
        for (i, model) in models.iter().enumerate() {
            if i == worldmodel_id {
                match *model.kind() {
                    ModelKind::Brush(ref bmodel) => {
                        world_renderer = Some(
                            BrushRendererBuilder::new(bmodel, gfx_pkg.clone())
                                .build()
                                .unwrap(),
                        );
                    }
                    _ => panic!("Invalid worldmodel"),
                }
            } else {
                match *model.kind() {
                    ModelKind::Brush(ref bmodel) => {
                        entity_renderers.push(EntityRenderer::Brush(
                            BrushRendererBuilder::new(bmodel, gfx_pkg.clone())
                                .build()
                                .unwrap(),
                        ));
                    }

                    _ => warn!("Non-brush renderers not implemented!"),
                    //_ => unimplemented!(),
                }
            }
        }

        let per_frame_bind_group_layout = gfx_pkg
            .device()
            .create_bind_group_layout(&PER_FRAME_BIND_GROUP_LAYOUT_DESCRIPTOR);

        let per_frame_bind_group = gfx_pkg
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("per-frame bind group"),
                layout: &per_frame_bind_group_layout,
                bindings: &[wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: &gfx_pkg.frame_uniform_buffer(),
                        range: 0..size_of::<FrameUniforms>() as wgpu::BufferAddress,
                    },
                }],
            });

        Renderer {
            gfx_pkg: gfx_pkg.clone(),
            world_renderer: world_renderer.unwrap(),
            entity_renderers,
            world_uniform_block,
            entity_uniform_blocks: RefCell::new(Vec::new()),
            per_frame_bind_group_layout,
            per_frame_bind_group,
        }
    }

    pub async fn update_uniform_buffers<'b, I>(
        &self,
        camera: &'b Camera,
        time: Duration,
        entities: I,
        lightstyle_values: &[f32],
    ) where
        I: Iterator<Item = &'b ClientEntity>,
    {
        let device = self.gfx_pkg.device();

        let mut frame_unif = self
            .gfx_pkg
            .frame_uniform_buffer()
            .map_write(0u64, size_of::<FrameUniforms>() as wgpu::BufferAddress)
            .await
            .unwrap();
        frame_unif.as_slice().copy_from_slice(unsafe {
            any_as_bytes(&FrameUniforms {
                lightmap_anim_frames: {
                    let mut frames = [0.0; 64];
                    frames.copy_from_slice(lightstyle_values);
                    frames
                },
                time: engine::duration_to_f32(time),
            })
        });

        let ent_buf = self.gfx_pkg.entity_uniform_buffer();
        ent_buf.map_write(device).await;
        let world_uniforms = EntityUniforms {
            transform: calculate_transform(
                camera,
                Vector3::zero(),
                Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
            ),
        };
        ent_buf.update(&self.world_uniform_block, world_uniforms);

        for (ent_pos, ent) in entities.into_iter().enumerate() {
            if ent_pos >= self.entity_uniform_blocks.borrow().len() {
                self.entity_uniform_blocks
                    .borrow_mut()
                    .push(ent_buf.allocate());
            }

            let ent_uniforms = EntityUniforms {
                transform: calculate_transform(camera, ent.origin, ent.angles),
            };

            ent_buf.update(&self.entity_uniform_blocks.borrow()[ent_pos], ent_uniforms);

            // TODO: if entity renderers have uniform buffers, update them here
            match self.renderer_for_entity(ent) {
                EntityRenderer::Brush(ref brush) => (),
                _ => (),
            }
        }
    }

    pub async fn render_pass<'b, I>(
        &self,
        color_attachment_view: &wgpu::TextureView,
        camera: &'b Camera,
        time: Duration,
        entities: I,
        lightstyle_values: &[f32],
    ) where
        I: Iterator<Item = &'b ClientEntity> + Clone,
    {
        let mut encoder = self
            .gfx_pkg
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        let depth_view = self.gfx_pkg.depth_attachment().create_default_view();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                attachment: color_attachment_view,
                resolve_target: None,
                load_op: wgpu::LoadOp::Clear,
                store_op: wgpu::StoreOp::Store,
                clear_color: wgpu::Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
            }],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachmentDescriptor {
                attachment: &depth_view,
                depth_load_op: wgpu::LoadOp::Clear,
                depth_store_op: wgpu::StoreOp::Store,
                clear_depth: 1.0,
                stencil_load_op: wgpu::LoadOp::Load,
                stencil_store_op: wgpu::StoreOp::Store,
                clear_stencil: 0,
            }),
        });

        self.update_uniform_buffers(camera, time, entities.clone(), lightstyle_values)
            .await;

        pass.set_bind_group(0, &self.per_frame_bind_group, &[]);

        self.world_renderer
            .record_draw(&mut pass, &self.world_uniform_block, None);
        for (ent_pos, ent) in entities.enumerate() {
            let model_id = ent.get_model_id();

            match self.renderer_for_entity(&ent) {
                EntityRenderer::Brush(ref bmodel) => bmodel.record_draw(
                    &mut pass,
                    &self.entity_uniform_blocks.borrow()[ent_pos],
                    None,
                ),
                _ => warn!("non-brush renderers not implemented!"),
                // _ => unimplemented!(),
            }
        }
    }

    fn renderer_for_entity(&self, ent: &ClientEntity) -> &EntityRenderer {
        // subtract 1 from index because world entity isn't counted
        &self.entity_renderers[ent.get_model_id() - 1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color_attachment(device: &wgpu::Device) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 1024,
                height: 768,
                depth: 1,
            },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_ATTACHMENT_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
        })
    }

    fn depth_attachment(device: &wgpu::Device) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 1024,
                height: 768,
                depth: 1,
            },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_ATTACHMENT_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
        })
    }

    fn palette() -> Palette {
        let rgb = [0u8; 768];
        Palette::new(&rgb)
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn graphics_package<'a>() -> GraphicsPackage<'a> {
        let adapter = wgpu::Adapter::request(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::Default,
                compatible_surface: None,
            },
            wgpu::BackendBit::PRIMARY,
        )
        .await
        .unwrap();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                extensions: wgpu::Extensions {
                    anisotropic_filtering: false,
                },
                limits: wgpu::Limits { max_bind_groups: 8 },
            })
            .await;

        let mut vfs = Vfs::new();
        // TODO don't require actual pakfiles for this test
        vfs.add_pakfile("id1/pak0.pak").unwrap();
        let gfx_pkg = GraphicsPackage::new(device, queue, 1366, 768, &vfs).unwrap();
        gfx_pkg
    }

    #[test]
    fn test_dynamic_uniform_buffer() {
        futures::executor::block_on(async {
            let gfx_pkg = graphics_package().await;
            let mut buf: DynamicUniformBuffer<u32> = DynamicUniformBuffer::new(&gfx_pkg.device());
            {
                let mut blocks = Vec::new();
                for i in 0..10 {
                    blocks.push(buf.allocate());
                }

                assert!(buf.clear().is_err());
                buf.map_write(gfx_pkg.device()).await;

                for (i, b) in blocks.iter().enumerate() {
                    buf.update(b, i as u32);
                }

                buf.unmap();

                buf.map_write(gfx_pkg.device()).await;

                for (i, b) in blocks.iter().enumerate() {
                    assert_eq!(buf.value(b), i as u32);
                }
            }
        });
    }
}
