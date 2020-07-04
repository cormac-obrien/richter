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

/// The `Pipeline` trait, which allows render pipelines to be defined more-or-less declaratively.

fn create_shader<S>(
    device: &wgpu::Device,
    compiler: &mut shaderc::Compiler,
    name: S,
    kind: shaderc::ShaderKind,
    source: S,
) -> wgpu::ShaderModule
where
    S: AsRef<str>,
{
    log::debug!("creating shader {}", name.as_ref());
    let spirv = compiler
        .compile_into_spirv(source.as_ref(), kind, name.as_ref(), "main", None)
        .unwrap();
    device.create_shader_module(wgpu::ShaderModuleSource::SpirV(spirv.as_binary()))
}

/// A trait describing the behavior of a render pipeline.
///
/// This trait's methods are used to define the pipeline's behavior in a more-or-less declarative
/// style, leaving the actual creation to the default implementation of `Pipeline::create()`.
///
/// In most cases, pipelines can simply be defined by implementing this trait on a unit struct.
pub trait Pipeline {
    /// The name of this pipeline.
    fn name() -> &'static str;

    /// The `BindGroupLayoutDescriptor`s describing the bindings used in the pipeline.
    fn bind_group_layout_descriptors() -> Vec<wgpu::BindGroupLayoutDescriptor<'static>>;

    /// The GLSL source of the pipeline's vertex shader.
    fn vertex_shader() -> &'static str;

    /// The GLSL source of the pipeline's fragment shader.
    fn fragment_shader() -> &'static str;

    /// The rasterization state descriptor used for the pipeline, if any.
    fn rasterization_state_descriptor() -> Option<wgpu::RasterizationStateDescriptor>;

    /// The primitive topology of the pipeline's vertex data.
    fn primitive_topology() -> wgpu::PrimitiveTopology;

    /// The color state used for the pipeline.
    fn color_state_descriptors() -> Vec<wgpu::ColorStateDescriptor>;

    /// The depth-stencil state used for the pipeline, if any.
    fn depth_stencil_state_descriptor() -> Option<wgpu::DepthStencilStateDescriptor>;

    /// Descriptors for the vertex buffers used by the pipeline.
    fn vertex_buffer_descriptors() -> Vec<wgpu::VertexBufferDescriptor<'static>>;

    /// Constructs a `RenderPipeline` and a list of `BindGroupLayout`s from the associated methods.
    ///
    /// `bind_group_layout_prefix` specifies a list of `BindGroupLayout`s to be prefixed onto those
    /// created from this pipeline's `bind_group_layout_descriptors()` method when creating the
    /// `RenderPipeline`. This permits the reuse of `BindGroupLayout`s between pipelines.
    fn create(
        device: &wgpu::Device,
        compiler: &mut shaderc::Compiler,
        bind_group_layout_prefix: &[wgpu::BindGroupLayout],
        sample_count: u32,
    ) -> (wgpu::RenderPipeline, Vec<wgpu::BindGroupLayout>) {
        info!("Creating {} pipeline", Self::name());
        let bind_group_layouts = Self::bind_group_layout_descriptors()
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

        let vertex_shader = create_shader(
            device,
            compiler,
            format!("{}.vert", Self::name()).as_str(),
            shaderc::ShaderKind::Vertex,
            Self::vertex_shader(),
        );
        let fragment_shader = create_shader(
            device,
            compiler,
            format!("{}.frag", Self::name()).as_str(),
            shaderc::ShaderKind::Fragment,
            Self::fragment_shader(),
        );

        info!("create_render_pipeline");
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
            rasterization_state: Self::rasterization_state_descriptor(),
            primitive_topology: Self::primitive_topology(),
            color_states: &Self::color_state_descriptors(),
            depth_stencil_state: Self::depth_stencil_state_descriptor(),
            vertex_state: wgpu::VertexStateDescriptor {
                index_format: wgpu::IndexFormat::Uint32,
                vertex_buffers: &Self::vertex_buffer_descriptors(),
            },
            sample_count,
            sample_mask: !0,
            alpha_to_coverage_enabled: false,
        });

        (pipeline, bind_group_layouts)
    }

    /// Reconstructs the pipeline using its original bind group layouts and a new sample count.
    ///
    /// Pipelines must be reconstructed when the MSAA sample count is changed.
    fn recreate(
        device: &wgpu::Device,
        compiler: &mut shaderc::Compiler,
        bind_group_layouts: &[&wgpu::BindGroupLayout],
        sample_count: u32,
    ) -> wgpu::RenderPipeline {
        let pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { bind_group_layouts });
        let vertex_shader = create_shader(
            device,
            compiler,
            format!("{}.vert", Self::name()).as_str(),
            shaderc::ShaderKind::Vertex,
            Self::vertex_shader(),
        );
        let fragment_shader = create_shader(
            device,
            compiler,
            format!("{}.frag", Self::name()).as_str(),
            shaderc::ShaderKind::Fragment,
            Self::fragment_shader(),
        );
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
            rasterization_state: Self::rasterization_state_descriptor(),
            primitive_topology: Self::primitive_topology(),
            color_states: &Self::color_state_descriptors(),
            depth_stencil_state: Self::depth_stencil_state_descriptor(),
            vertex_state: wgpu::VertexStateDescriptor {
                index_format: wgpu::IndexFormat::Uint32,
                vertex_buffers: &Self::vertex_buffer_descriptors(),
            },
            sample_count,
            sample_mask: !0,
            alpha_to_coverage_enabled: false,
        });

        pipeline
    }
}
