use std::mem::size_of;

use crate::{
    client::render::{pipeline::Pipeline, ui::quad::QuadPipeline, GraphicsState},
    common::util::any_as_bytes,
};

use cgmath::{Vector4, Zero};

lazy_static! {
    pub static ref BIND_GROUP_LAYOUT_DESCRIPTOR_BINDINGS: [Vec<wgpu::BindGroupLayoutEntry>; 1] = [
        vec![
            // sampler
            wgpu::BindGroupLayoutEntry::new(
                0,
                wgpu::ShaderStage::FRAGMENT,
                wgpu::BindingType::Sampler { comparison: false },
            ),

            // color buffer
            wgpu::BindGroupLayoutEntry::new(
                1,
                wgpu::ShaderStage::FRAGMENT,
                wgpu::BindingType::SampledTexture {
                    dimension: wgpu::TextureViewDimension::D2,
                    component_type: wgpu::TextureComponentType::Float,
                    multisampled: true,
                },
            ),

            // PostProcessUniforms
            wgpu::BindGroupLayoutEntry::new(
                2,
                wgpu::ShaderStage::FRAGMENT,
                wgpu::BindingType::UniformBuffer {
                    dynamic: false,
                    min_binding_size: Some(std::num::NonZeroU64::new(
                        size_of::<PostProcessUniforms>() as u64
                    ).unwrap()),
                },
            ),
        ]
    ];
}

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
        let uniform_buffer = device.create_buffer_with_data(
            unsafe {
                any_as_bytes(&PostProcessUniforms {
                    color_shift: [0.0; 4],
                })
            },
            wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
        );

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

impl Pipeline for PostProcessPipeline {
    fn name() -> &'static str {
        "postprocess"
    }

    fn bind_group_layout_descriptors() -> Vec<wgpu::BindGroupLayoutDescriptor<'static>> {
        vec![wgpu::BindGroupLayoutDescriptor {
            label: Some("postprocess bind group"),
            bindings: &BIND_GROUP_LAYOUT_DESCRIPTOR_BINDINGS[0],
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

    fn rasterization_state_descriptor() -> Option<wgpu::RasterizationStateDescriptor> {
        QuadPipeline::rasterization_state_descriptor()
    }

    fn primitive_topology() -> wgpu::PrimitiveTopology {
        QuadPipeline::primitive_topology()
    }

    fn color_state_descriptors() -> Vec<wgpu::ColorStateDescriptor> {
        QuadPipeline::color_state_descriptors()
    }

    fn depth_stencil_state_descriptor() -> Option<wgpu::DepthStencilStateDescriptor> {
        None
    }

    fn vertex_buffer_descriptors() -> Vec<wgpu::VertexBufferDescriptor<'static>> {
        QuadPipeline::vertex_buffer_descriptors()
    }
}

pub struct PostProcessRenderer {
    bind_group: wgpu::BindGroup,
}

impl PostProcessRenderer {
    pub fn new(state: &GraphicsState, color_buffer: &wgpu::TextureView) -> PostProcessRenderer {
        let bind_group = state
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("postprocess bind group"),
                layout: &state.postprocess_pipeline().bind_group_layouts()[0],
                bindings: &[
                    // sampler
                    wgpu::Binding {
                        binding: 0,
                        // TODO: might need a dedicated sampler if downsampling
                        resource: wgpu::BindingResource::Sampler(state.diffuse_sampler()),
                    },
                    // color buffer
                    wgpu::Binding {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(color_buffer),
                    },
                    // uniform buffer
                    wgpu::Binding {
                        binding: 2,
                        resource: wgpu::BindingResource::Buffer(
                            state.postprocess_pipeline().uniform_buffer().slice(..),
                        ),
                    },
                ],
            });

        PostProcessRenderer { bind_group }
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
        debug!("PostProcessRenderer::record_draw");
        self.update_uniform_buffers(state, color_shift);
        pass.set_pipeline(state.postprocess_pipeline().pipeline());
        pass.set_vertex_buffer(0, state.quad_vertex_buffer().slice(..));
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..6, 0..1);
    }
}
