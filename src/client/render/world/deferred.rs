use std::{mem::size_of, num::NonZeroU64};

use cgmath::{Matrix4, SquareMatrix as _, Vector3, Zero as _};

use crate::{
    client::{
        entity::MAX_LIGHTS,
        render::{pipeline::Pipeline, ui::quad::QuadPipeline, GraphicsState},
    },
    common::util::any_as_bytes,
};

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PointLight {
    pub origin: Vector3<f32>,
    pub radius: f32,
}

#[repr(C, align(256))]
#[derive(Clone, Copy, Debug)]
pub struct DeferredUniforms {
    pub inv_projection: [[f32; 4]; 4],
    pub light_count: u32,
    pub _pad: [u32; 3],
    pub lights: [PointLight; MAX_LIGHTS],
}

pub struct DeferredPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layouts: Vec<wgpu::BindGroupLayout>,
    uniform_buffer: wgpu::Buffer,
}

impl DeferredPipeline {
    pub fn new(
        device: &wgpu::Device,
        compiler: &mut shaderc::Compiler,
        sample_count: u32,
    ) -> DeferredPipeline {
        let (pipeline, bind_group_layouts) =
            DeferredPipeline::create(device, compiler, &[], sample_count);

        use wgpu::util::DeviceExt as _;
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: unsafe {
                any_as_bytes(&DeferredUniforms {
                    inv_projection: Matrix4::identity().into(),
                    light_count: 0,
                    _pad: [0; 3],
                    lights: [PointLight {
                        origin: Vector3::zero(),
                        radius: 0.0,
                    }; MAX_LIGHTS],
                })
            },
            usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
        });

        DeferredPipeline {
            pipeline,
            bind_group_layouts,
            uniform_buffer,
        }
    }

    pub fn rebuild(
        &mut self,
        device: &wgpu::Device,
        compiler: &mut shaderc::Compiler,
        sample_count: u32,
    ) {
        let layout_refs: Vec<_> = self.bind_group_layouts.iter().collect();
        let pipeline = DeferredPipeline::recreate(device, compiler, &layout_refs, sample_count);
        self.pipeline = pipeline;
    }

    pub fn pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }

    pub fn bind_group_layouts(&self) -> &[wgpu::BindGroupLayout] {
        &self.bind_group_layouts
    }

    pub fn uniform_buffer(&self) -> &wgpu::Buffer {
        &self.uniform_buffer
    }
}

const BIND_GROUP_LAYOUT_ENTRIES: &[wgpu::BindGroupLayoutEntry] = &[
    // sampler
    wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStage::FRAGMENT,
        ty: wgpu::BindingType::Sampler {
            filtering: true,
            comparison: false,
        },
        count: None,
    },
    // color buffer
    wgpu::BindGroupLayoutEntry {
        binding: 1,
        visibility: wgpu::ShaderStage::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            view_dimension: wgpu::TextureViewDimension::D2,
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            multisampled: true,
        },
        count: None,
    },
    // normal buffer
    wgpu::BindGroupLayoutEntry {
        binding: 2,
        visibility: wgpu::ShaderStage::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            view_dimension: wgpu::TextureViewDimension::D2,
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            multisampled: true,
        },
        count: None,
    },
    // light buffer
    wgpu::BindGroupLayoutEntry {
        binding: 3,
        visibility: wgpu::ShaderStage::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            view_dimension: wgpu::TextureViewDimension::D2,
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            multisampled: true,
        },
        count: None,
    },
    // depth buffer
    wgpu::BindGroupLayoutEntry {
        binding: 4,
        visibility: wgpu::ShaderStage::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            view_dimension: wgpu::TextureViewDimension::D2,
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            multisampled: true,
        },
        count: None,
    },
    // uniform buffer
    wgpu::BindGroupLayoutEntry {
        binding: 5,
        visibility: wgpu::ShaderStage::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: NonZeroU64::new(size_of::<DeferredUniforms>() as u64),
        },
        count: None,
    },
];

impl Pipeline for DeferredPipeline {
    type VertexPushConstants = ();
    type SharedPushConstants = ();
    type FragmentPushConstants = ();

    fn name() -> &'static str {
        "deferred"
    }

    fn bind_group_layout_descriptors() -> Vec<wgpu::BindGroupLayoutDescriptor<'static>> {
        vec![wgpu::BindGroupLayoutDescriptor {
            label: Some("deferred bind group"),
            entries: BIND_GROUP_LAYOUT_ENTRIES,
        }]
    }

    fn vertex_shader() -> &'static str {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/shaders/deferred.vert"
        ))
    }

    fn fragment_shader() -> &'static str {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/shaders/deferred.frag"
        ))
    }

    fn primitive_state() -> wgpu::PrimitiveState {
        QuadPipeline::primitive_state()
    }

    fn color_target_states() -> Vec<wgpu::ColorTargetState> {
        QuadPipeline::color_target_states()
    }

    fn depth_stencil_state() -> Option<wgpu::DepthStencilState> {
        None
    }

    fn vertex_buffer_layouts() -> Vec<wgpu::VertexBufferLayout<'static>> {
        QuadPipeline::vertex_buffer_layouts()
    }
}

pub struct DeferredRenderer {
    bind_group: wgpu::BindGroup,
}

impl DeferredRenderer {
    fn create_bind_group(
        state: &GraphicsState,
        diffuse_buffer: &wgpu::TextureView,
        normal_buffer: &wgpu::TextureView,
        light_buffer: &wgpu::TextureView,
        depth_buffer: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        state
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("deferred bind group"),
                layout: &state.deferred_pipeline().bind_group_layouts()[0],
                entries: &[
                    // sampler
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Sampler(state.diffuse_sampler()),
                    },
                    // diffuse buffer
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(diffuse_buffer),
                    },
                    // normal buffer
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(normal_buffer),
                    },
                    // light buffer
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(light_buffer),
                    },
                    // depth buffer
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(depth_buffer),
                    },
                    // uniform buffer
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: state.deferred_pipeline().uniform_buffer(),
                            offset: 0,
                            size: None,
                        }),
                    },
                ],
            })
    }

    pub fn new(
        state: &GraphicsState,
        diffuse_buffer: &wgpu::TextureView,
        normal_buffer: &wgpu::TextureView,
        light_buffer: &wgpu::TextureView,
        depth_buffer: &wgpu::TextureView,
    ) -> DeferredRenderer {
        let bind_group = Self::create_bind_group(
            state,
            diffuse_buffer,
            normal_buffer,
            light_buffer,
            depth_buffer,
        );

        DeferredRenderer { bind_group }
    }

    pub fn rebuild(
        &mut self,
        state: &GraphicsState,
        diffuse_buffer: &wgpu::TextureView,
        normal_buffer: &wgpu::TextureView,
        light_buffer: &wgpu::TextureView,
        depth_buffer: &wgpu::TextureView,
    ) {
        self.bind_group = Self::create_bind_group(
            state,
            diffuse_buffer,
            normal_buffer,
            light_buffer,
            depth_buffer,
        );
    }

    pub fn update_uniform_buffers(&self, state: &GraphicsState, uniforms: DeferredUniforms) {
        // update color shift
        state
            .queue()
            .write_buffer(state.deferred_pipeline().uniform_buffer(), 0, unsafe {
                any_as_bytes(&uniforms)
            });
    }

    pub fn record_draw<'pass>(
        &'pass self,
        state: &'pass GraphicsState,
        pass: &mut wgpu::RenderPass<'pass>,
        uniforms: DeferredUniforms,
    ) {
        self.update_uniform_buffers(state, uniforms);
        pass.set_pipeline(state.deferred_pipeline().pipeline());
        pass.set_vertex_buffer(0, state.quad_pipeline().vertex_buffer().slice(..));
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..6, 0..1);
    }
}
