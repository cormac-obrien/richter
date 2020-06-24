use std::mem::size_of;

use crate::{
    client::render::wgpu::{
        ui::{
            layout::{Anchor, ScreenPosition},
            screen_space_vertex_transform,
        },
        uniform::DynamicUniformBufferBlock,
        GraphicsState, Pipeline, TextureData, COLOR_ATTACHMENT_FORMAT, DEPTH_ATTACHMENT_FORMAT,
    },
    common::wad::QPic,
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

pub struct QuadPipeline;

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
        r#"
#version 450

layout(location = 0) in vec2 a_position;
layout(location = 1) in vec2 a_texcoord;

layout(location = 0) out vec2 f_texcoord;

layout(set = 2, binding = 0) uniform QuadUniforms {
    mat4 transform;
} quad_uniforms;

void main() {
    f_texcoord = a_texcoord;
    gl_Position = quad_uniforms.transform * vec4(a_position, 0.0, 1.0);
}
"#
    }

    fn fragment_shader() -> &'static str {
        r#"
#version 450

layout(location = 0) in vec2 f_texcoord;

layout(location = 0) out vec4 color_attachment;

layout(set = 0, binding = 0) uniform sampler quad_sampler;
layout(set = 1, binding = 0) uniform texture2D quad_texture;

void main() {
    color_attachment = texture(sampler2D(quad_texture, quad_sampler), f_texcoord);
}
"#
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
            format: COLOR_ATTACHMENT_FORMAT,
            alpha_blend: wgpu::BlendDescriptor::REPLACE,
            color_blend: wgpu::BlendDescriptor::REPLACE,
            write_mask: wgpu::ColorWrite::ALL,
        }]
    }

    fn depth_stencil_state_descriptor() -> Option<wgpu::DepthStencilStateDescriptor> {
        Some(wgpu::DepthStencilStateDescriptor {
            format: DEPTH_ATTACHMENT_FORMAT,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Always,
            stencil_front: wgpu::StencilStateFaceDescriptor::IGNORE,
            stencil_back: wgpu::StencilStateFaceDescriptor::IGNORE,
            stencil_read_mask: 0,
            stencil_write_mask: 0,
        })
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
    texture: wgpu::Texture,
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
                layout: &state.quad_bind_group_layouts()[1],
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
}

/// Specifies what size a quad should be when rendered on the screen.
#[derive(Clone, Copy, Debug)]
pub enum QuadSize {
    /// Render the quad at an exact size in pixels.
    Absolute {
        /// The width of the quad in pixels.
        width: u32,

        /// The height of the quad in pixels.
        height: u32,
    },

    /// Render the quad at a size specified relative to the dimensions of its texture.
    Scale {
        /// The factor to multiply by the quad's texture dimensions to determine its size.
        factor: f32,
    },

    /// Render the quad at a size specified relative to the size of the display.
    DisplayScale {
        /// The ratio of the display size at which to render the quad.
        ratio: f32,
    },
}

impl QuadSize {
    pub fn to_wh(
        &self,
        texture_width: u32,
        texture_height: u32,
        display_width: u32,
        display_height: u32,
    ) -> (u32, u32) {
        match *self {
            QuadSize::Absolute { width, height } => (width, height),
            QuadSize::Scale { factor } => (
                (texture_width as f32 * factor) as u32,
                (texture_height as f32 * factor) as u32,
            ),
            QuadSize::DisplayScale { ratio } => (
                (display_width as f32 * ratio) as u32,
                (display_height as f32 * ratio) as u32,
            ),
        }
    }
}

/// A command which specifies how a quad should be rendered.
pub struct QuadRendererCommand<'a> {
    /// The texture to be mapped to the quad.
    pub texture: &'a QuadTexture,

    /// The position of the quad on the screen.
    pub position: ScreenPosition,

    /// Which part of the quad to position at `position`.
    pub anchor: Anchor,

    /// The size at which to render the quad.
    pub size: QuadSize,
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
                layout: &state.quad_bind_group_layouts()[0],
                bindings: &[wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(state.diffuse_sampler()),
                }],
            });
        let transform_bind_group = state
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("quad transform bind group"),
                layout: &state.quad_bind_group_layouts()[2],
                bindings: &[wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(
                        state.quad_uniform_buffer().buffer().slice(..),
                    ),
                }],
            });

        QuadRenderer {
            sampler_bind_group,
            transform_bind_group,
        }
    }

    pub fn generate_uniforms<'cmds>(
        &self,
        commands: &[QuadRendererCommand<'cmds>],
        display_width: u32,
        display_height: u32,
    ) -> Vec<QuadUniforms> {
        let mut uniforms = Vec::new();

        for cmd in commands {
            let QuadRendererCommand {
                texture,
                position,
                anchor,
                size,
            } = *cmd;

            let (screen_x, screen_y) = position.to_xy(display_width, display_height);
            let (quad_x, quad_y) = anchor.to_xy(texture.width, texture.height);
            let x = screen_x - quad_x;
            let y = screen_y - quad_y;
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

    pub fn record_draw<'state, 'pass, 'cmds>(
        &'pass self,
        state: &'pass GraphicsState<'state>,
        pass: &mut wgpu::RenderPass<'pass>,
        cmds: &'pass [QuadRendererCommand<'pass>],
        blocks: &'cmds [DynamicUniformBufferBlock<'state, QuadUniforms>],
    ) {
        pass.set_pipeline(state.quad_pipeline());
        pass.set_vertex_buffer(0, state.quad_vertex_buffer().slice(..));
        pass.set_bind_group(0, &self.sampler_bind_group, &[]);
        for (cmd, block) in cmds.iter().zip(blocks.iter()) {
            pass.set_bind_group(1, &cmd.texture.bind_group, &[]);
            pass.set_bind_group(2, &self.transform_bind_group, &[block.offset()]);
            pass.draw(0..6, 0..1);
        }
    }
}
