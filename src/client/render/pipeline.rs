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

use std::mem::size_of;

use crate::common::util::{any_as_bytes, Pod};

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
    device.create_shader_module(&wgpu::ShaderModuleDescriptor {
        label: Some(name.as_ref()),
        source: wgpu::ShaderSource::SpirV(spirv.as_binary().into()),
        flags: wgpu::ShaderFlags::empty(),
    })
}

pub enum PushConstantUpdate<T> {
    /// Update the push constant to a new value.
    Update(T),
    /// Retain the current value of the push constant.
    Retain,
    /// Clear the push constant to no value.
    Clear,
}

/// A trait describing the behavior of a render pipeline.
///
/// This trait's methods are used to define the pipeline's behavior in a more-or-less declarative
/// style, leaving the actual creation to the default implementation of `Pipeline::create()`.
pub trait Pipeline {
    /// Push constants used for the vertex stage of the pipeline.
    type VertexPushConstants: Pod;

    /// Push constants shared between the vertex and fragment stages of the pipeline.
    type SharedPushConstants: Pod;

    /// Push constants used for the fragment stage of the pipeline.
    type FragmentPushConstants: Pod;

    /// The name of this pipeline.
    fn name() -> &'static str;

    /// The `BindGroupLayoutDescriptor`s describing the bindings used in the pipeline.
    fn bind_group_layout_descriptors() -> Vec<wgpu::BindGroupLayoutDescriptor<'static>>;

    /// The GLSL source of the pipeline's vertex shader.
    fn vertex_shader() -> &'static str;

    /// The GLSL source of the pipeline's fragment shader.
    fn fragment_shader() -> &'static str;

    /// The primitive state used for rasterization in this pipeline.
    fn primitive_state() -> wgpu::PrimitiveState;

    /// The color state used for the pipeline.
    fn color_target_states() -> Vec<wgpu::ColorTargetState>;

    /// The depth-stencil state used for the pipeline, if any.
    fn depth_stencil_state() -> Option<wgpu::DepthStencilState>;

    /// Descriptors for the vertex buffers used by the pipeline.
    fn vertex_buffer_layouts() -> Vec<wgpu::VertexBufferLayout<'static>>;

    fn vertex_push_constant_range() -> wgpu::PushConstantRange {
        let range = wgpu::PushConstantRange {
            stages: wgpu::ShaderStage::VERTEX,
            range: 0..size_of::<Self::VertexPushConstants>() as u32
                + size_of::<Self::SharedPushConstants>() as u32,
        };
        debug!("vertex push constant range: {:#?}", &range);
        range
    }

    fn fragment_push_constant_range() -> wgpu::PushConstantRange {
        let range = wgpu::PushConstantRange {
            stages: wgpu::ShaderStage::FRAGMENT,
            range: size_of::<Self::VertexPushConstants>() as u32
                ..size_of::<Self::VertexPushConstants>() as u32
                    + size_of::<Self::SharedPushConstants>() as u32
                    + size_of::<Self::FragmentPushConstants>() as u32,
        };
        debug!("fragment push constant range: {:#?}", &range);
        range
    }

    fn push_constant_ranges() -> Vec<wgpu::PushConstantRange> {
        let vpc_size = size_of::<Self::VertexPushConstants>();
        let spc_size = size_of::<Self::SharedPushConstants>();
        let fpc_size = size_of::<Self::FragmentPushConstants>();

        match (vpc_size, spc_size, fpc_size) {
            (0, 0, 0) => Vec::new(),
            (_, 0, 0) => vec![Self::vertex_push_constant_range()],
            (0, 0, _) => vec![Self::fragment_push_constant_range()],
            _ => vec![
                Self::vertex_push_constant_range(),
                Self::fragment_push_constant_range(),
            ],
        }
    }

    /// Ensures that the associated push constant types have the proper size and
    /// alignment.
    fn validate_push_constant_types(limits: wgpu::Limits) {
        let pc_alignment = wgpu::PUSH_CONSTANT_ALIGNMENT as usize;
        let max_pc_size = limits.max_push_constant_size as usize;
        let vpc_size = size_of::<Self::VertexPushConstants>();
        let spc_size = size_of::<Self::SharedPushConstants>();
        let fpc_size = size_of::<Self::FragmentPushConstants>();
        assert_eq!(
            vpc_size % pc_alignment,
            0,
            "Vertex push constant size must be a multiple of {} bytes",
            wgpu::PUSH_CONSTANT_ALIGNMENT,
        );
        assert_eq!(
            spc_size % pc_alignment,
            0,
            "Shared push constant size must be a multiple of {} bytes",
            wgpu::PUSH_CONSTANT_ALIGNMENT,
        );
        assert_eq!(
            fpc_size % pc_alignment,
            0,
            "Fragment push constant size must be a multiple of {} bytes",
            wgpu::PUSH_CONSTANT_ALIGNMENT,
        );
        assert!(
            vpc_size + spc_size + fpc_size <= max_pc_size,
            "Combined size of push constants must be less or equal than push constant size limit of {}",
            max_pc_size
        );
    }

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
        Self::validate_push_constant_types(device.limits());

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
            let ranges = Self::push_constant_ranges();
            let label = format!("{} pipeline layout", Self::name());
            let desc = wgpu::PipelineLayoutDescriptor {
                label: Some(&label),
                bind_group_layouts: &layouts,
                push_constant_ranges: &ranges,
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
            label: Some(&format!("{} pipeline", Self::name())),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vertex_shader,
                entry_point: "main",
                buffers: &Self::vertex_buffer_layouts(),
            },
            primitive: Self::primitive_state(),
            fragment: Some(wgpu::FragmentState {
                module: &fragment_shader,
                entry_point: "main",
                targets: &Self::color_target_states(),
            }),
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            depth_stencil: Self::depth_stencil_state(),
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
        Self::validate_push_constant_types(device.limits());

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(&format!("{} pipeline layout", Self::name())),
            bind_group_layouts,
            push_constant_ranges: &[
                Self::vertex_push_constant_range(),
                Self::fragment_push_constant_range(),
            ],
        });
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
            label: Some(&format!("{} pipeline", Self::name())),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vertex_shader,
                entry_point: "main",
                buffers: &Self::vertex_buffer_layouts(),
            },
            primitive: Self::primitive_state(),
            fragment: Some(wgpu::FragmentState {
                module: &fragment_shader,
                entry_point: "main",
                targets: &Self::color_target_states(),
            }),
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            depth_stencil: Self::depth_stencil_state(),
        });

        pipeline
    }

    /// Set the push constant data for a render pass.
    ///
    /// For each argument, if the value is `Some`, then the corresponding push
    /// constant range is updated. If the value is `None`, the corresponding push
    /// constant range is cleared.
    fn set_push_constants<'a>(
        pass: &mut wgpu::RenderPass<'a>,
        vpc: PushConstantUpdate<&'a Self::VertexPushConstants>,
        spc: PushConstantUpdate<&'a Self::SharedPushConstants>,
        fpc: PushConstantUpdate<&'a Self::FragmentPushConstants>,
    ) {
        use PushConstantUpdate::*;

        let vpc_offset = 0;
        let spc_offset = vpc_offset + size_of::<Self::VertexPushConstants>() as u32;
        let fpc_offset = spc_offset + size_of::<Self::SharedPushConstants>() as u32;

        // these push constant size checks are known statically and will be
        // compiled out

        if size_of::<Self::VertexPushConstants>() > 0 {
            let data = match vpc {
                Update(v) => Some(unsafe { any_as_bytes(v) }),
                Retain => None,
                Clear => Some(&[][..]),
            };

            if let Some(d) = data {
                trace!(
                    "Update vertex push constants at offset {} with data {:?}",
                    vpc_offset,
                    data
                );

                pass.set_push_constants(wgpu::ShaderStage::VERTEX, vpc_offset, d);
            }
        }

        if size_of::<Self::SharedPushConstants>() > 0 {
            let data = match spc {
                Update(s) => Some(unsafe { any_as_bytes(s) }),
                Retain => None,
                Clear => Some(&[][..]),
            };

            if let Some(d) = data {
                trace!(
                    "Update shared push constants at offset {} with data {:?}",
                    spc_offset,
                    data
                );

                pass.set_push_constants(
                    wgpu::ShaderStage::VERTEX | wgpu::ShaderStage::FRAGMENT,
                    spc_offset,
                    d,
                );
            }
        }

        if size_of::<Self::FragmentPushConstants>() > 0 {
            let data = match fpc {
                Update(f) => Some(unsafe { any_as_bytes(f) }),
                Retain => None,
                Clear => Some(&[][..]),
            };

            if let Some(d) = data {
                trace!(
                    "Update fragment push constants at offset {} with data {:?}",
                    fpc_offset,
                    data
                );

                pass.set_push_constants(wgpu::ShaderStage::FRAGMENT, fpc_offset, d);
            }
        }
    }
}
