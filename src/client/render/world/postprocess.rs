use crate::client::render::{pipeline::Pipeline, ui::quad::QuadPipeline, GraphicsState};

lazy_static! {
    pub static ref BIND_GROUP_LAYOUT_DESCRIPTOR_BINDINGS: [Vec<wgpu::BindGroupLayoutEntry>; 1] = [
        vec![
            // sampler
            wgpu::BindGroupLayoutEntry::new(
                0,
                wgpu::ShaderStage::FRAGMENT,
                wgpu::BindingType::Sampler { comparison: false }
            ),

            // color buffer
            wgpu::BindGroupLayoutEntry::new(
                1,
                wgpu::ShaderStage::FRAGMENT,
                wgpu::BindingType::SampledTexture {
                    dimension: wgpu::TextureViewDimension::D2,
                    component_type: wgpu::TextureComponentType::Float,
                    multisampled: true,
                }
            ),
        ]
    ];
}

pub struct PostProcessPipeline;

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
                layout: &state.postprocess_bind_group_layouts()[0],
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
                ],
            });

        PostProcessRenderer { bind_group }
    }

    pub fn record_draw<'pass>(
        &'pass self,
        state: &'pass GraphicsState,
        pass: &mut wgpu::RenderPass<'pass>,
    ) {
        debug!("PostProcessRenderer::record_draw");
        pass.set_pipeline(state.postprocess_pipeline());
        pass.set_vertex_buffer(0, state.quad_vertex_buffer().slice(..));
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..6, 0..1);
    }
}
