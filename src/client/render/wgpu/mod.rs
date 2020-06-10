// mod atlas;
mod brush;
mod error;
mod palette;
mod uniform;
mod warp;

pub use error::{RenderError, RenderErrorKind};
pub use palette::Palette;

use std::{
    borrow::Cow,
    cell::{Cell, Ref, RefCell, RefMut},
    marker::PhantomData,
    mem::size_of,
    rc::Rc,
};

use crate::{
    client::{
        render::wgpu::{
            brush::{BrushRenderer, BrushRendererBuilder},
            uniform::{DynamicUniformBuffer, DynamicUniformBufferBlock},
        },
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

use cgmath::{Deg, Euler, Matrix4, SquareMatrix, Vector3, Vector4, Zero};
use chrono::Duration;
use failure::{Error, Fail};
use shaderc::{CompileOptions, Compiler};
use strum::IntoEnumIterator;

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

// uniform float array elements are aligned as if they were vec4s
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug)]
pub struct UniformArrayFloat {
    value: f32,
}

#[repr(C, align(256))]
#[derive(Copy, Clone)]
// TODO: derive Debug once const generics are stable
pub struct FrameUniforms {
    lightmap_anim_frames: [UniformArrayFloat; 64],
    camera_pos: Vector4<f32>,
    time: f32,
}

#[repr(C, align(256))]
#[derive(Clone, Copy, Debug)]
pub struct EntityUniforms {
    transform: Matrix4<f32>,
}

pub struct GraphicsPackage<'a> {
    device: wgpu::Device,
    queue: wgpu::Queue,
    depth_attachment: RefCell<wgpu::Texture>,
    frame_uniform_buffer: wgpu::Buffer,
    entity_uniform_buffer: RefCell<DynamicUniformBuffer<'a, EntityUniforms>>,
    brush_texture_uniform_buffer: RefCell<DynamicUniformBuffer<'a, brush::TextureUniforms>>,
    brush_texture_uniform_blocks: Vec<DynamicUniformBufferBlock<'a, brush::TextureUniforms>>,
    per_frame_bind_group: wgpu::BindGroup,
    per_frame_bind_group_layout: wgpu::BindGroupLayout,
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
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_ATTACHMENT_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
        }));

        let frame_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame uniform buffer"),
            size: size_of::<FrameUniforms>() as wgpu::BufferAddress,
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

        let per_frame_bind_group_layout =
            device.create_bind_group_layout(&PER_FRAME_BIND_GROUP_LAYOUT_DESCRIPTOR);

        let per_frame_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("per-frame bind group"),
            layout: &per_frame_bind_group_layout,
            bindings: &[wgpu::Binding {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(frame_uniform_buffer.slice(..)),
            }],
        });

        let (brush_pipeline, brush_bind_group_layouts) =
            brush::create_render_pipeline(&device, &per_frame_bind_group_layout);

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
            compare: wgpu::CompareFunction::Undefined,
            anisotropy_clamp: 0,
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
            compare: wgpu::CompareFunction::Undefined,
            anisotropy_clamp: 0,
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
            brush_texture_uniform_buffer,
            brush_texture_uniform_blocks,
            per_frame_bind_group_layout,
            per_frame_bind_group,
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
            mapped_at_creation: false,
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
        self.queue.submit(vec![cmd_buffer]);

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
        &self.brush_bind_group_layouts[id as usize - 1]
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
    None,
}

/// Top-level renderer.
pub struct Renderer<'a> {
    gfx_pkg: Rc<GraphicsPackage<'a>>,

    // TODO: make this a proper WorldRenderer with visibility rendering
    world_renderer: BrushRenderer<'a>,
    entity_renderers: Vec<EntityRenderer<'a>>,

    world_uniform_block: DynamicUniformBufferBlock<'a, EntityUniforms>,
    entity_uniform_blocks: RefCell<Vec<DynamicUniformBufferBlock<'a, EntityUniforms>>>,
}

impl<'a> Renderer<'a> {
    pub fn new(
        models: &[Model],
        worldmodel_id: usize,
        gfx_pkg: Rc<GraphicsPackage<'a>>,
    ) -> Renderer<'a> {
        let mut world_renderer = None;
        let mut entity_renderers = Vec::new();

        let world_uniform_block = gfx_pkg
            .entity_uniform_buffer_mut()
            .allocate(EntityUniforms {
                transform: Matrix4::identity(),
            });

        for (i, model) in models.iter().enumerate() {
            if i == worldmodel_id {
                match *model.kind() {
                    ModelKind::Brush(ref bmodel) => {
                        world_renderer = Some(
                            BrushRendererBuilder::new(bmodel, gfx_pkg.clone(), true)
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
                            BrushRendererBuilder::new(bmodel, gfx_pkg.clone(), false)
                                .build()
                                .unwrap(),
                        ));
                    }

                    _ => {
                        warn!("Non-brush renderers not implemented!");
                        entity_renderers.push(EntityRenderer::None);
                    } //_ => unimplemented!(),
                }
            }
        }

        Renderer {
            gfx_pkg: gfx_pkg.clone(),
            world_renderer: world_renderer.unwrap(),
            entity_renderers,
            world_uniform_block,
            entity_uniform_blocks: RefCell::new(Vec::new()),
        }
    }

    pub fn update_uniform_buffers<'b, I>(
        &'b self,
        camera: &Camera,
        time: Duration,
        entities: I,
        lightstyle_values: &[f32],
    ) where
        I: Iterator<Item = &'b ClientEntity>,
    {
        let _guard = flame::start_guard("Renderer::update_uniform");

        let device = self.gfx_pkg.device();

        println!("time = {:?}", engine::duration_to_f32(time));
        trace!("Updating frame uniform buffer");
        self.gfx_pkg
            .queue()
            .write_buffer(self.gfx_pkg.frame_uniform_buffer(), 0, unsafe {
                any_as_bytes(&FrameUniforms {
                    lightmap_anim_frames: {
                        let mut frames = [UniformArrayFloat { value: 0.0 }; 64];
                        for i in 0..64 {
                            frames[i].value = lightstyle_values[i];
                        }
                        frames
                    },
                    camera_pos: camera.origin.extend(1.0),
                    time: engine::duration_to_f32(time),
                })
            });

        trace!("Updating entity uniform buffer");
        let queue = self.gfx_pkg.queue();
        let world_uniforms = EntityUniforms {
            transform: calculate_transform(
                camera,
                Vector3::zero(),
                Vector3::new(Deg(0.0), Deg(0.0), Deg(0.0)),
            ),
        };
        self.gfx_pkg
            .entity_uniform_buffer_mut()
            .write_block(&self.world_uniform_block, world_uniforms);

        for (ent_pos, ent) in entities.into_iter().enumerate() {
            let ent_uniforms = EntityUniforms {
                transform: calculate_transform(camera, ent.origin, ent.angles),
            };

            if ent_pos >= self.entity_uniform_blocks.borrow().len() {
                // if we don't have enough blocks, get a new one
                let block = self
                    .gfx_pkg
                    .entity_uniform_buffer_mut()
                    .allocate(ent_uniforms);
                self.entity_uniform_blocks.borrow_mut().push(block);
            } else {
                self.gfx_pkg
                    .entity_uniform_buffer_mut()
                    .write_block(&self.entity_uniform_blocks.borrow()[ent_pos], ent_uniforms);
            }

            // TODO: if entity renderers have uniform buffers, update them here
            match self.renderer_for_entity(ent) {
                EntityRenderer::Brush(ref brush) => (),
                EntityRenderer::None => (),
            }
        }

        self.gfx_pkg
            .entity_uniform_buffer()
            .flush(self.gfx_pkg.queue());
    }

    pub fn render_pass<'b, I>(
        &'b self,
        color_attachment_view: &wgpu::TextureView,
        camera: &Camera,
        time: Duration,
        entities: I,
        lightstyle_values: &[f32],
    ) where
        I: Iterator<Item = &'b ClientEntity> + Clone,
    {
        let _guard = flame::start_guard("Renderer::render_pass");
        let mut encoder = self
            .gfx_pkg
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        let depth_view = self.gfx_pkg.depth_attachment().create_default_view();
        {
            info!("Updating uniform buffers");
            self.update_uniform_buffers(camera, time, entities.clone(), lightstyle_values);

            info!("Beginning render pass");
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
                    depth_read_only: false,
                    clear_depth: 1.0,
                    stencil_load_op: wgpu::LoadOp::Load,
                    stencil_store_op: wgpu::StoreOp::Store,
                    stencil_read_only: false,
                    clear_stencil: 0,
                }),
            });

            pass.set_bind_group(0, &self.gfx_pkg.per_frame_bind_group, &[]);

            info!("Drawing world");
            self.world_renderer
                .record_draw(&mut pass, &self.world_uniform_block, camera);
            for (ent_pos, ent) in entities.enumerate() {
                let model_id = ent.get_model_id();

                match self.renderer_for_entity(&ent) {
                    EntityRenderer::Brush(ref bmodel) => bmodel.record_draw(
                        &mut pass,
                        &self.entity_uniform_blocks.borrow()[ent_pos],
                        camera,
                    ),
                    _ => warn!("non-brush renderers not implemented!"),
                    // _ => unimplemented!(),
                }
            }
        }

        let command_buffer = encoder.finish();
        {
            let _submit_guard = flame::start_guard("Submit and poll");
            self.gfx_pkg.queue().submit(vec![command_buffer]);
            self.gfx_pkg.device().poll(wgpu::Maintain::Wait);
        }
    }

    fn renderer_for_entity(&self, ent: &ClientEntity) -> &EntityRenderer<'a> {
        // subtract 1 from index because world entity isn't counted
        &self.entity_renderers[ent.get_model_id() - 1]
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::common::util::any_slice_as_bytes;

    const TEST_VERTEX_BUFFER_DESCRIPTOR: wgpu::VertexBufferDescriptor =
        wgpu::VertexBufferDescriptor {
            stride: size_of::<Vector3<f32>>() as u64,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: &[wgpu::VertexAttributeDescriptor {
                offset: 0,
                format: wgpu::VertexFormat::Float3,
                shader_location: 0,
            }],
        };

    fn color_attachment(device: &wgpu::Device) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 1024,
                height: 768,
                depth: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_ATTACHMENT_FORMAT,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
        })
    }

    fn palette() -> Palette {
        let rgb = [0u8; 768];
        Palette::new(&rgb)
    }
}
