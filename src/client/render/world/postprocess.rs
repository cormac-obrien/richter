use std::{mem::size_of, num::NonZeroU64};

use crate::{
    client::render::{pipeline::Pipeline, ui::quad::QuadPipeline, GraphicsState},
    common::util::any_as_bytes,
};

#[repr(C, align(256))]
#[derive(Clone, Copy, Debug)]
pub struct PostProcessUniforms {
    pub color_shift: [f32; 4],
}

pub struct PostProcessPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layouts: Vec<wgpu::BindGroupLayout>,
    uniform_buffer: wgpu::Buffer,
}

impl PostProcessPipeline {
    pub fn new(
        device: &wgpu::Device,
        compiler: &mut shaderc::Compiler,
        sample_count: u32,
    ) -> PostProcessPipeline {
        let (pipeline, bind_group_layouts) =
            PostProcessPipeline::create(device, compiler, &[], sample_count);
        use wgpu::util::DeviceExt as _;
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: unsafe {
                any_as_bytes(&PostProcessUniforms {
                    color_shift: [0.0; 4],
                })
            },
            usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
        });

        PostProcessPipeline {
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
        let pipeline = PostProcessPipeline::recreate(device, compiler, &layout_refs, sample_count);
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
    // PostProcessUniforms
    wgpu::BindGroupLayoutEntry {
        binding: 2,
        visibility: wgpu::ShaderStage::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: NonZeroU64::new(size_of::<PostProcessUniforms>() as u64),
        },
        count: None,
    },
];

impl Pipeline for PostProcessPipeline {
    type VertexPushConstants = ();
    type SharedPushConstants = ();
    type FragmentPushConstants = ();

    fn name() -> &'static str {
        "postprocess"
    }

    fn bind_group_layout_descriptors() -> Vec<wgpu::BindGroupLayoutDescriptor<'static>> {
        vec![wgpu::BindGroupLayoutDescriptor {
            label: Some("postprocess bind group"),
            entries: BIND_GROUP_LAYOUT_ENTRIES,
        }]
    }

    fn vertex_shader() -> &'static str {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/shaders/postprocess.vert"
        ))
    }

    fn fragment_shader() -> &'static str {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/shaders/postprocess.frag"
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

pub struct PostProcessRenderer {
    bind_group: wgpu::BindGroup,
}

impl PostProcessRenderer {
    pub fn create_bind_group(
        state: &GraphicsState,
        color_buffer: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        state
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("postprocess bind group"),
                layout: &state.postprocess_pipeline().bind_group_layouts()[0],
                entries: &[
                    // sampler
                    wgpu::BindGroupEntry {
                        binding: 0,
                        // TODO: might need a dedicated sampler if downsampling
                        resource: wgpu::BindingResource::Sampler(state.diffuse_sampler()),
                    },
                    // color buffer
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(color_buffer),
                    },
                    // uniform buffer
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: state.postprocess_pipeline().uniform_buffer(),
                            offset: 0,
                            size: None,
                        }),
                    },
                ],
            })
    }

    pub fn new(state: &GraphicsState, color_buffer: &wgpu::TextureView) -> PostProcessRenderer {
        let bind_group = Self::create_bind_group(state, color_buffer);

        PostProcessRenderer { bind_group }
    }

    pub fn rebuild(&mut self, state: &GraphicsState, color_buffer: &wgpu::TextureView) {
        self.bind_group = Self::create_bind_group(state, color_buffer);
    }

    pub fn update_uniform_buffers(&self, state: &GraphicsState, color_shift: [f32; 4]) {
        // update color shift
        state
            .queue()
            .write_buffer(state.postprocess_pipeline().uniform_buffer(), 0, unsafe {
                any_as_bytes(&PostProcessUniforms { color_shift })
            });
    }

    pub fn record_draw<'pass>(
        &'pass self,
        state: &'pass GraphicsState,
        pass: &mut wgpu::RenderPass<'pass>,
        color_shift: [f32; 4],
    ) {
        self.update_uniform_buffers(state, color_shift);
        pass.set_pipeline(state.postprocess_pipeline().pipeline());
        pass.set_vertex_buffer(0, state.quad_pipeline().vertex_buffer().slice(..));
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..6, 0..1);
    }
}
