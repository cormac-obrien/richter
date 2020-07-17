use std::mem::size_of;

use crate::{
    client::{
        entity::particle::Particle,
        render::{
            create_texture,
            pipeline::Pipeline,
            world::{Camera, WorldPipelineBase},
            Palette, TextureData,
        },
    },
    common::{math::Angles, util::any_slice_as_bytes},
};

use bumpalo::Bump;
use cgmath::Matrix4;

lazy_static! {
    static ref BIND_GROUP_LAYOUT_DESCRIPTOR_BINDINGS: [Vec<wgpu::BindGroupLayoutEntry>; 1] = [
        vec![
            wgpu::BindGroupLayoutEntry::new(
                0,
                wgpu::ShaderStage::FRAGMENT,
                wgpu::BindingType::Sampler { comparison: false },
            ),
            // per-index texture array
            wgpu::BindGroupLayoutEntry {
                count: Some(256),
                ..wgpu::BindGroupLayoutEntry::new(
                    1,
                    wgpu::ShaderStage::FRAGMENT,
                    wgpu::BindingType::SampledTexture {
                        dimension: wgpu::TextureViewDimension::D2,
                        component_type: wgpu::TextureComponentType::Float,
                        multisampled: false,
                    },
                )
            },
        ]
    ];

    static ref VERTEX_BUFFER_DESCRIPTOR_ATTRIBUTES: [Vec<wgpu::VertexAttributeDescriptor>; 2] = [
        wgpu::vertex_attr_array![
            // position
            0 => Float3,
            // texcoord
            1 => Float2,
        ].to_vec(),
        wgpu::vertex_attr_array![
            // instance color (index)
            2 => Uint,
        ].to_vec(),
    ];
}

#[rustfmt::skip]
const PARTICLE_TEXTURE_PIXELS: [u8; 64] = [
    0, 0, 1, 1, 1, 1, 0, 0,
    0, 1, 1, 1, 1, 1, 1, 0,
    1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1,
    0, 1, 1, 1, 1, 1, 1, 0,
    0, 0, 1, 1, 1, 1, 0, 0,
];

pub struct ParticlePipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layouts: Vec<wgpu::BindGroupLayout>,
    vertex_buffer: wgpu::Buffer,
    sampler: wgpu::Sampler,
    textures: Vec<wgpu::Texture>,
    texture_views: Vec<wgpu::TextureView>,
    bind_group: wgpu::BindGroup,
}

impl ParticlePipeline {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        compiler: &mut shaderc::Compiler,
        sample_count: u32,
        palette: &Palette,
    ) -> ParticlePipeline {
        let (pipeline, bind_group_layouts) =
            ParticlePipeline::create(device, compiler, &[], sample_count);

        let vertex_buffer = device.create_buffer_with_data(
            unsafe { any_slice_as_bytes(&VERTICES) },
            wgpu::BufferUsage::VERTEX,
        );

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("particle sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            lod_min_clamp: -1000.0,
            lod_max_clamp: 1000.0,
            compare: None,
            anisotropy_clamp: Some(16),
        });

        let textures: Vec<wgpu::Texture> = (0..256)
            .map(|i| {
                let mut pixels = PARTICLE_TEXTURE_PIXELS;

                // set up palette translation
                for pix in pixels.iter_mut() {
                    if *pix == 0 {
                        *pix = 0xFF;
                    } else {
                        *pix *= i as u8;
                    }
                }

                let (diffuse_data, _) = palette.translate(&pixels);

                create_texture(
                    device,
                    queue,
                    Some(&format!("particle texture {}", i)),
                    8,
                    8,
                    &TextureData::Diffuse(diffuse_data),
                )
            })
            .collect();
        let texture_views: Vec<wgpu::TextureView> =
            textures.iter().map(|t| t.create_default_view()).collect();

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("particle bind group"),
            layout: &bind_group_layouts[0],
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureViewArray(&texture_views[..]),
                },
            ],
        });

        ParticlePipeline {
            pipeline,
            bind_group_layouts,
            sampler,
            textures,
            texture_views,
            bind_group,
            vertex_buffer,
        }
    }

    pub fn rebuild(
        &mut self,
        device: &wgpu::Device,
        compiler: &mut shaderc::Compiler,
        sample_count: u32,
    ) {
        let layout_refs: Vec<_> = self.bind_group_layouts.iter().collect();
        self.pipeline = ParticlePipeline::recreate(device, compiler, &layout_refs, sample_count);
    }

    pub fn pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }

    pub fn bind_group_layouts(&self) -> &[wgpu::BindGroupLayout] {
        &self.bind_group_layouts
    }

    pub fn vertex_buffer(&self) -> &wgpu::Buffer {
        &self.vertex_buffer
    }

    pub fn record_draw<'a, 'b, P>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        bump: &'a Bump,
        camera: &Camera,
        particles: P,
    ) where
        P: Iterator<Item = &'b Particle>,
    {
        pass.set_pipeline(self.pipeline());
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_bind_group(0, &self.bind_group, &[]);

        // face toward camera
        let Angles { pitch, yaw, roll } = camera.angles();
        let rotation = Angles {
            pitch: -pitch,
            yaw: -yaw,
            roll: -roll,
        }
        .mat4_wgpu();

        for particle in particles {
            let q_origin = particle.origin();
            let translation =
                Matrix4::from_translation([-q_origin.y, q_origin.z, -q_origin.x].into());
            Self::set_push_constants(
                pass,
                Some(bump.alloc(VertexPushConstants {
                    transform: camera.view_projection() * translation * rotation,
                })),
                None,
                Some(bump.alloc(FragmentPushConstants {
                    color: particle.color() as u32,
                })),
            );

            pass.draw(0..6, 0..1);
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct VertexPushConstants {
    pub transform: Matrix4<f32>,
}

#[derive(Copy, Clone, Debug)]
pub struct FragmentPushConstants {
    pub color: u32,
}

impl Pipeline for ParticlePipeline {
    type VertexPushConstants = VertexPushConstants;
    type SharedPushConstants = ();
    type FragmentPushConstants = FragmentPushConstants;

    fn name() -> &'static str {
        "particle"
    }

    fn vertex_shader() -> &'static str {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/shaders/particle.vert"
        ))
    }

    fn fragment_shader() -> &'static str {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/shaders/particle.frag"
        ))
    }

    // NOTE: if any of the binding indices are changed, they must also be changed in
    // the corresponding shaders and the BindGroupLayout generation functions.
    fn bind_group_layout_descriptors() -> Vec<wgpu::BindGroupLayoutDescriptor<'static>> {
        vec![
            // group 0
            wgpu::BindGroupLayoutDescriptor {
                label: Some("particle bind group layout"),
                entries: &BIND_GROUP_LAYOUT_DESCRIPTOR_BINDINGS[0],
            },
        ]
    }

    fn rasterization_state_descriptor() -> Option<wgpu::RasterizationStateDescriptor> {
        WorldPipelineBase::rasterization_state_descriptor()
    }

    fn primitive_topology() -> wgpu::PrimitiveTopology {
        wgpu::PrimitiveTopology::TriangleList
    }

    fn color_state_descriptors() -> Vec<wgpu::ColorStateDescriptor> {
        WorldPipelineBase::color_state_descriptors()
    }

    fn depth_stencil_state_descriptor() -> Option<wgpu::DepthStencilStateDescriptor> {
        let mut desc = WorldPipelineBase::depth_stencil_state_descriptor().unwrap();
        desc.depth_write_enabled = false;
        Some(desc)
    }

    // NOTE: if the vertex format is changed, this descriptor must also be changed accordingly.
    fn vertex_buffer_descriptors() -> Vec<wgpu::VertexBufferDescriptor<'static>> {
        vec![
            wgpu::VertexBufferDescriptor {
                stride: size_of::<ParticleVertex>() as u64,
                step_mode: wgpu::InputStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![
                    // position
                    0 => Float3,
                    // texcoord
                    1 => Float2,
                ],
            },
            wgpu::VertexBufferDescriptor {
                stride: size_of::<ParticleInstance>() as u64,
                step_mode: wgpu::InputStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![
                    // instance position
                    2 => Float3,
                    // color index
                    3 => Uint,
                ],
            },
        ]
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct ParticleVertex {
    position: [f32; 3],
    texcoord: [f32; 2],
}

pub const VERTICES: [ParticleVertex; 6] = [
    ParticleVertex {
        position: [-1.0, -1.0, 0.0],
        texcoord: [0.0, 1.0],
    },
    ParticleVertex {
        position: [-1.0, 1.0, 0.0],
        texcoord: [0.0, 0.0],
    },
    ParticleVertex {
        position: [1.0, 1.0, 0.0],
        texcoord: [1.0, 0.0],
    },
    ParticleVertex {
        position: [-1.0, -1.0, 0.0],
        texcoord: [0.0, 1.0],
    },
    ParticleVertex {
        position: [1.0, 1.0, 0.0],
        texcoord: [1.0, 0.0],
    },
    ParticleVertex {
        position: [1.0, -1.0, 0.0],
        texcoord: [1.0, 1.0],
    },
];

#[repr(C)]
pub struct ParticleInstance {
    color: u32,
}
