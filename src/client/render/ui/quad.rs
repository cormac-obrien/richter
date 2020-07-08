use std::{
    cell::{Ref, RefCell, RefMut},
    mem::size_of,
};

use crate::{
    client::render::{
        ui::{
            layout::{Layout, Size},
            screen_space_vertex_transform,
        },
        uniform::{self, DynamicUniformBuffer, DynamicUniformBufferBlock},
        Extent2d, GraphicsState, Pipeline, TextureData, DIFFUSE_ATTACHMENT_FORMAT,
    },
    common::{util::any_slice_as_bytes, wad::QPic},
};

use cgmath::Matrix4;

pub const VERTICES: [QuadVertex; 6] = [
    QuadVertex {
        position: [0.0, 0.0],
        texcoord: [0.0, 1.0],
    },
    QuadVertex {
        position: [0.0, 1.0],
        texcoord: [0.0, 0.0],
    },
    QuadVertex {
        position: [1.0, 1.0],
        texcoord: [1.0, 0.0],
    },
    QuadVertex {
        position: [0.0, 0.0],
        texcoord: [0.0, 1.0],
    },
    QuadVertex {
        position: [1.0, 1.0],
        texcoord: [1.0, 0.0],
    },
    QuadVertex {
        position: [1.0, 0.0],
        texcoord: [1.0, 1.0],
    },
];

// these type aliases are here to aid readability of e.g. size_of::<Position>()
pub type Position = [f32; 2];
pub type Texcoord = [f32; 2];

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct QuadVertex {
    position: Position,
    texcoord: Texcoord,
}

lazy_static! {
    pub static ref BIND_GROUP_LAYOUT_DESCRIPTOR_BINDINGS: [Vec<wgpu::BindGroupLayoutEntry>; 3] = [
        vec![
            // sampler
            wgpu::BindGroupLayoutEntry::new(
                0,
                wgpu::ShaderStage::FRAGMENT,
                wgpu::BindingType::Sampler { comparison: false },
            ),
        ],
        vec![
            // texture
            wgpu::BindGroupLayoutEntry::new(
                0,
                wgpu::ShaderStage::FRAGMENT,
                wgpu::BindingType::SampledTexture {
                    dimension: wgpu::TextureViewDimension::D2,
                    component_type: wgpu::TextureComponentType::Float,
                    multisampled: false,
                },
            ),
        ],
        vec![
            // transform matrix
            // TODO: move to push constants once they're exposed in wgpu
            wgpu::BindGroupLayoutEntry::new(
                0,
                wgpu::ShaderStage::all(),
                wgpu::BindingType::UniformBuffer {
                    dynamic: true,
                    min_binding_size: Some(
                        std::num::NonZeroU64::new(size_of::<QuadUniforms>() as u64).unwrap(),
                    ),
                },
            ),
        ],
    ];

    static ref VERTEX_BUFFER_DESCRIPTOR_ATTRIBUTES: Vec<wgpu::VertexAttributeDescriptor> = vec![
        // position
        wgpu::VertexAttributeDescriptor {
            offset: 0,
            format: wgpu::VertexFormat::Float2,
            shader_location: 0,
        },
        // diffuse texcoord
        wgpu::VertexAttributeDescriptor {
            offset: size_of::<Position>() as u64,
            format: wgpu::VertexFormat::Float2,
            shader_location: 1,
        },
    ];
}

pub struct QuadPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layouts: Vec<wgpu::BindGroupLayout>,
    vertex_buffer: wgpu::Buffer,
    uniform_buffer: RefCell<DynamicUniformBuffer<QuadUniforms>>,
    uniform_buffer_blocks: RefCell<Vec<DynamicUniformBufferBlock<QuadUniforms>>>,
}

impl QuadPipeline {
    pub fn new(
        device: &wgpu::Device,
        compiler: &mut shaderc::Compiler,
        sample_count: u32,
    ) -> QuadPipeline {
        let (pipeline, bind_group_layouts) =
            QuadPipeline::create(device, compiler, &[], sample_count);

        let vertex_buffer = device.create_buffer_with_data(
            unsafe { any_slice_as_bytes(&VERTICES) },
            wgpu::BufferUsage::VERTEX,
        );

        let uniform_buffer = RefCell::new(DynamicUniformBuffer::new(device));
        let uniform_buffer_blocks = RefCell::new(Vec::new());

        QuadPipeline {
            pipeline,
            bind_group_layouts,
            vertex_buffer,
            uniform_buffer,
            uniform_buffer_blocks,
        }
    }

    pub fn rebuild(
        &mut self,
        device: &wgpu::Device,
        compiler: &mut shaderc::Compiler,
        sample_count: u32,
    ) {
        let layout_refs = self.bind_group_layouts.iter().collect::<Vec<_>>();
        self.pipeline = QuadPipeline::recreate(device, compiler, &layout_refs, sample_count);
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

    pub fn uniform_buffer(&self) -> Ref<DynamicUniformBuffer<QuadUniforms>> {
        self.uniform_buffer.borrow()
    }

    pub fn uniform_buffer_mut(&self) -> RefMut<DynamicUniformBuffer<QuadUniforms>> {
        self.uniform_buffer.borrow_mut()
    }

    pub fn uniform_buffer_blocks(&self) -> Ref<Vec<DynamicUniformBufferBlock<QuadUniforms>>> {
        self.uniform_buffer_blocks.borrow()
    }

    pub fn uniform_buffer_blocks_mut(
        &self,
    ) -> RefMut<Vec<DynamicUniformBufferBlock<QuadUniforms>>> {
        self.uniform_buffer_blocks.borrow_mut()
    }
}

impl Pipeline for QuadPipeline {
    fn name() -> &'static str {
        "quad"
    }

    fn bind_group_layout_descriptors() -> Vec<wgpu::BindGroupLayoutDescriptor<'static>> {
        vec![
            // group 0: per-frame
            wgpu::BindGroupLayoutDescriptor {
                label: Some("per-frame quad bind group"),
                bindings: &BIND_GROUP_LAYOUT_DESCRIPTOR_BINDINGS[0],
            },
            // group 1: per-texture
            wgpu::BindGroupLayoutDescriptor {
                label: Some("per-texture quad bind group"),
                bindings: &BIND_GROUP_LAYOUT_DESCRIPTOR_BINDINGS[1],
            },
            // group 2: per-quad
            wgpu::BindGroupLayoutDescriptor {
                label: Some("per-texture quad bind group"),
                bindings: &BIND_GROUP_LAYOUT_DESCRIPTOR_BINDINGS[2],
            },
        ]
    }

    fn vertex_shader() -> &'static str {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/shaders/quad.vert"))
    }

    fn fragment_shader() -> &'static str {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/shaders/quad.frag"))
    }

    fn rasterization_state_descriptor() -> Option<wgpu::RasterizationStateDescriptor> {
        Some(wgpu::RasterizationStateDescriptor {
            front_face: wgpu::FrontFace::Cw,
            cull_mode: wgpu::CullMode::Back,
            depth_bias: 0,
            depth_bias_slope_scale: 0.0,
            depth_bias_clamp: 0.0,
        })
    }

    fn primitive_topology() -> wgpu::PrimitiveTopology {
        wgpu::PrimitiveTopology::TriangleList
    }

    fn color_state_descriptors() -> Vec<wgpu::ColorStateDescriptor> {
        vec![wgpu::ColorStateDescriptor {
            format: DIFFUSE_ATTACHMENT_FORMAT,
            alpha_blend: wgpu::BlendDescriptor::REPLACE,
            color_blend: wgpu::BlendDescriptor::REPLACE,
            write_mask: wgpu::ColorWrite::ALL,
        }]
    }

    fn depth_stencil_state_descriptor() -> Option<wgpu::DepthStencilStateDescriptor> {
        None
    }

    // NOTE: if the vertex format is changed, this descriptor must also be changed accordingly.
    fn vertex_buffer_descriptors() -> Vec<wgpu::VertexBufferDescriptor<'static>> {
        vec![wgpu::VertexBufferDescriptor {
            stride: size_of::<QuadVertex>() as u64,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: &VERTEX_BUFFER_DESCRIPTOR_ATTRIBUTES[..],
        }]
    }
}

#[repr(C, align(256))]
#[derive(Clone, Copy, Debug)]
pub struct QuadUniforms {
    transform: Matrix4<f32>,
}

pub struct QuadTexture {
    #[allow(dead_code)]
    texture: wgpu::Texture,
    #[allow(dead_code)]
    texture_view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}

impl QuadTexture {
    pub fn from_qpic(state: &GraphicsState, qpic: &QPic) -> QuadTexture {
        let (diffuse_data, _) = state.palette().translate(qpic.indices());
        let texture = state.create_texture(
            None,
            qpic.width(),
            qpic.height(),
            &TextureData::Diffuse(diffuse_data),
        );
        let texture_view = texture.create_default_view();
        let bind_group = state
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &state.quad_pipeline().bind_group_layouts()[1],
                bindings: &[wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                }],
            });

        QuadTexture {
            texture,
            texture_view,
            bind_group,
            width: qpic.width(),
            height: qpic.height(),
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn scale_width(&self, scale: f32) -> u32 {
        (self.width as f32 * scale) as u32
    }

    pub fn scale_height(&self, scale: f32) -> u32 {
        (self.height as f32 * scale) as u32
    }
}

/// A command which specifies how a quad should be rendered.
pub struct QuadRendererCommand<'a> {
    /// The texture to be mapped to the quad.
    pub texture: &'a QuadTexture,

    /// The layout specifying the size and position of the quad on the screen.
    pub layout: Layout,
}

pub struct QuadRenderer {
    sampler_bind_group: wgpu::BindGroup,
    transform_bind_group: wgpu::BindGroup,
}

impl QuadRenderer {
    pub fn new(state: &GraphicsState) -> QuadRenderer {
        let sampler_bind_group = state
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("quad sampler bind group"),
                layout: &state.quad_pipeline().bind_group_layouts()[0],
                bindings: &[wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(state.diffuse_sampler()),
                }],
            });
        let transform_bind_group = state
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("quad transform bind group"),
                layout: &state.quad_pipeline().bind_group_layouts()[2],
                bindings: &[wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(
                        state.quad_pipeline().uniform_buffer().buffer().slice(..),
                    ),
                }],
            });

        QuadRenderer {
            sampler_bind_group,
            transform_bind_group,
        }
    }

    fn generate_uniforms<'cmds>(
        &self,
        commands: &[QuadRendererCommand<'cmds>],
        target_size: Extent2d,
    ) -> Vec<QuadUniforms> {
        let mut uniforms = Vec::new();

        for cmd in commands {
            let QuadRendererCommand {
                texture,
                layout:
                    Layout {
                        position,
                        anchor,
                        size,
                    },
            } = *cmd;

            let scale = match size {
                Size::Scale { factor } => factor,
                _ => 1.0,
            };

            let Extent2d {
                width: display_width,
                height: display_height,
            } = target_size;

            let (screen_x, screen_y) = position.to_xy(display_width, display_height, scale);
            let (quad_x, quad_y) = anchor.to_xy(texture.width, texture.height);
            let x = screen_x - (quad_x as f32 * scale) as i32;
            let y = screen_y - (quad_y as f32 * scale) as i32;
            let (quad_width, quad_height) =
                size.to_wh(texture.width, texture.height, display_width, display_height);

            uniforms.push(QuadUniforms {
                transform: screen_space_vertex_transform(
                    display_width,
                    display_height,
                    quad_width,
                    quad_height,
                    x,
                    y,
                ),
            });
        }

        uniforms
    }

    pub fn record_draw<'pass, 'cmds>(
        &'pass self,
        state: &'pass GraphicsState,
        pass: &mut wgpu::RenderPass<'pass>,
        target_size: Extent2d,
        commands: &'pass [QuadRendererCommand<'pass>],
    ) {
        // update uniform buffer
        let uniforms = self.generate_uniforms(commands, target_size);
        uniform::clear_and_rewrite(
            state.queue(),
            &mut state.quad_pipeline().uniform_buffer_mut(),
            &mut state.quad_pipeline().uniform_buffer_blocks_mut(),
            &uniforms,
        );

        pass.set_pipeline(state.quad_pipeline().pipeline());
        pass.set_vertex_buffer(0, state.quad_pipeline().vertex_buffer().slice(..));
        pass.set_bind_group(0, &self.sampler_bind_group, &[]);
        for (cmd, block) in commands
            .iter()
            .zip(state.quad_pipeline().uniform_buffer_blocks().iter())
        {
            pass.set_bind_group(1, &cmd.texture.bind_group, &[]);
            pass.set_bind_group(2, &self.transform_bind_group, &[block.offset()]);
            pass.draw(0..6, 0..1);
        }
    }
}
