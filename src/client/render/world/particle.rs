use std::{
    mem::size_of,
    num::{NonZeroU32, NonZeroU8},
};

use crate::{
    client::{
        entity::particle::Particle,
        render::{
            create_texture,
            pipeline::{Pipeline, PushConstantUpdate},
            world::{Camera, WorldPipelineBase},
            Palette, TextureData,
        },
    },
    common::{math::Angles, util::any_slice_as_bytes},
};

use bumpalo::Bump;
use cgmath::Matrix4;

lazy_static! {
    static ref VERTEX_BUFFER_ATTRIBUTES: [Vec<wgpu::VertexAttribute>; 1] = [
        wgpu::vertex_attr_array![
            // position
            0 => Float32x3,
            // texcoord
            1 => Float32x2,
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

        use wgpu::util::DeviceExt as _;
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: unsafe { any_slice_as_bytes(&VERTICES) },
            usage: wgpu::BufferUsage::VERTEX,
        });

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
            anisotropy_clamp: NonZeroU8::new(16),
            border_color: None,
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
        let texture_views: Vec<wgpu::TextureView> = textures
            .iter()
            .map(|t| t.create_view(&Default::default()))
            .collect();
        let texture_view_refs = texture_views.iter().collect::<Vec<_>>();

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
                    resource: wgpu::BindingResource::TextureViewArray(&texture_view_refs[..]),
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
        use PushConstantUpdate::*;

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
                Update(bump.alloc(VertexPushConstants {
                    transform: camera.view_projection() * translation * rotation,
                })),
                Retain,
                Update(bump.alloc(FragmentPushConstants {
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

const BIND_GROUP_LAYOUT_ENTRIES: &[wgpu::BindGroupLayoutEntry] = &[
    wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStage::FRAGMENT,
        ty: wgpu::BindingType::Sampler {
            filtering: true,
            comparison: false,
        },
        count: None,
    },
    // per-index texture array
    wgpu::BindGroupLayoutEntry {
        binding: 1,
        visibility: wgpu::ShaderStage::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            view_dimension: wgpu::TextureViewDimension::D2,
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            multisampled: false,
        },
        count: NonZeroU32::new(256),
    },
];

lazy_static! {
    static ref VERTEX_ATTRIBUTES: [[wgpu::VertexAttribute; 2]; 2] = [
        wgpu::vertex_attr_array![
            // position
            0 => Float32x3,
            // texcoord
            1 => Float32x2,
        ],
        wgpu::vertex_attr_array![
            // instance position
            2 => Float32x3,
            // color index
            3 => Uint32,
        ]
    ];
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
                entries: BIND_GROUP_LAYOUT_ENTRIES,
            },
        ]
    }

    fn primitive_state() -> wgpu::PrimitiveState {
        WorldPipelineBase::primitive_state()
    }

    fn color_target_states() -> Vec<wgpu::ColorTargetState> {
        WorldPipelineBase::color_target_states()
    }

    fn depth_stencil_state() -> Option<wgpu::DepthStencilState> {
        let mut desc = WorldPipelineBase::depth_stencil_state().unwrap();
        desc.depth_write_enabled = false;
        Some(desc)
    }

    // NOTE: if the vertex format is changed, this descriptor must also be changed accordingly.
    fn vertex_buffer_layouts() -> Vec<wgpu::VertexBufferLayout<'static>> {
        vec![wgpu::VertexBufferLayout {
            array_stride: size_of::<ParticleVertex>() as u64,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: &VERTEX_ATTRIBUTES[0],
        }]
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
