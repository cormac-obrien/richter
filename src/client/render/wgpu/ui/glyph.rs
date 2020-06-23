use std::mem::size_of;

use crate::client::render::wgpu::{
    ui::{
        layout::{Anchor, ScreenPosition},
        quad::QuadPipeline,
        screen_space_vertex_transform,
    },
    uniform::DynamicUniformBufferBlock,
    GraphicsState, Pipeline, TextureData,
};

use cgmath::Matrix4;

pub const GLYPH_WIDTH: usize = 8;
pub const GLYPH_HEIGHT: usize = 8;
const GLYPH_COLS: usize = 16;
const GLYPH_ROWS: usize = 16;
const GLYPH_COUNT: usize = GLYPH_ROWS * GLYPH_COLS;
const GLYPH_TEXTURE_WIDTH: usize = GLYPH_WIDTH * GLYPH_COLS;
const GLYPH_TEXTURE_HEIGHT: usize = GLYPH_HEIGHT * GLYPH_ROWS;

lazy_static! {
    static ref BIND_GROUP_LAYOUT_DESCRIPTOR_BINDINGS: [Vec<wgpu::BindGroupLayoutEntry>; 2] = [
        // group 0: constant for all glyph draws
        vec![
            // sampler
            wgpu::BindGroupLayoutEntry::new(
                0,
                wgpu::ShaderStage::FRAGMENT,
                wgpu::BindingType::Sampler { comparison: false }
            ),
            // glyph texture array
            wgpu::BindGroupLayoutEntry {
                count: Some(GLYPH_COUNT as u32),
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
        ],

        // group 1: per-glyph
        vec![
            // GlyphUniforms
            wgpu::BindGroupLayoutEntry::new(
                0,
                wgpu::ShaderStage::all(),
                wgpu::BindingType::UniformBuffer {
                    dynamic: true,
                    min_binding_size: Some(
                        std::num::NonZeroU64::new(size_of::<GlyphUniforms>() as u64).unwrap(),
                    ),
                },
            ),
        ],
    ];
}

#[repr(C, align(256))]
#[derive(Clone, Copy, Debug)]
pub struct GlyphUniforms {
    transform: Matrix4<f32>,
    layer: u32,
}

pub struct GlyphPipeline;

impl Pipeline for GlyphPipeline {
    fn name() -> &'static str {
        "glyph"
    }

    fn vertex_shader() -> &'static str {
        r#"
#version 450

layout(location = 0) in vec2 a_position;
layout(location = 1) in vec2 a_texcoord;

layout(location = 0) out vec2 f_texcoord;

layout(set = 1, binding = 0) uniform GlyphUniforms {
    mat4 transform;
    uint layer;
} u_glyph;

void main() {
    f_texcoord = a_texcoord;
    gl_Position = u_glyph.transform * vec4(a_position, 0.0, 1.0);
}
"#
    }

    fn fragment_shader() -> &'static str {
        r#"
#version 450

layout(location = 0) in vec2 f_texcoord;

layout(location = 0) out vec4 output_attachment;

layout(set = 0, binding = 0) uniform sampler u_sampler;
layout(set = 0, binding = 1) uniform texture2D u_texture[256];

layout(set = 1, binding = 0) uniform GlyphUniforms {
    mat4 transform;
    uint layer;
} u_glyph;

void main() {
    vec4 color = texture(sampler2D(u_texture[u_glyph.layer], u_sampler), f_texcoord);
    if (color.a == 0) {
        discard;
    } else {
        output_attachment = color;
    }
}
"#
    }

    fn bind_group_layout_descriptors() -> Vec<wgpu::BindGroupLayoutDescriptor<'static>> {
        vec![
            wgpu::BindGroupLayoutDescriptor {
                label: Some("glyph constant bind group"),
                bindings: &BIND_GROUP_LAYOUT_DESCRIPTOR_BINDINGS[0],
            },
            wgpu::BindGroupLayoutDescriptor {
                label: Some("glyph per_draw bind group"),
                bindings: &BIND_GROUP_LAYOUT_DESCRIPTOR_BINDINGS[1],
            },
        ]
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
        QuadPipeline::depth_stencil_state_descriptor()
    }

    fn vertex_buffer_descriptors() -> Vec<wgpu::VertexBufferDescriptor<'static>> {
        QuadPipeline::vertex_buffer_descriptors()
    }
}

pub enum GlyphRendererCommand {
    Glyph {
        glyph_id: u8,
        position: ScreenPosition,
        anchor: Anchor,
    },
    Text {
        text: String,
        position: ScreenPosition,
        anchor: Anchor,
    },
}

pub struct GlyphRenderer {
    textures: Vec<wgpu::Texture>,
    texture_views: Vec<wgpu::TextureView>,
    const_bind_group: wgpu::BindGroup,
    per_draw_bind_group: wgpu::BindGroup,
}

impl GlyphRenderer {
    pub fn new(state: &GraphicsState) -> GlyphRenderer {
        assert!(state
            .device()
            .capabilities()
            .contains(wgpu::Capabilities::SAMPLED_TEXTURE_BINDING_ARRAY));
        let conchars = state.gfx_wad().open_conchars().unwrap();

        // TODO: validate conchars dimensions

        let indices = conchars
            .indices()
            .iter()
            .map(|i| if *i == 0 { 0xFF } else { *i })
            .collect::<Vec<_>>();

        // reorder indices from atlas order to array order
        let mut array_order = Vec::new();
        for glyph_id in 0..GLYPH_COUNT {
            for glyph_r in 0..GLYPH_HEIGHT {
                for glyph_c in 0..GLYPH_WIDTH {
                    let atlas_r = GLYPH_HEIGHT * (glyph_id / GLYPH_COLS) + glyph_r;
                    let atlas_c = GLYPH_WIDTH * (glyph_id % GLYPH_COLS) + glyph_c;
                    array_order.push(indices[atlas_r * GLYPH_TEXTURE_WIDTH + atlas_c]);
                }
            }
        }

        let textures = array_order
            .chunks_exact(GLYPH_WIDTH * GLYPH_HEIGHT)
            .enumerate()
            .map(|(id, indices)| {
                let (diffuse_data, _) = state.palette().translate(&indices);
                state.create_texture(
                    Some(&format!("conchars[{}]", id)),
                    GLYPH_WIDTH as u32,
                    GLYPH_HEIGHT as u32,
                    &TextureData::Diffuse(diffuse_data),
                )
            })
            .collect::<Vec<_>>();

        let texture_views = textures
            .iter()
            .map(|tex| tex.create_default_view())
            .collect::<Vec<_>>();

        let const_bind_group = state
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("glyph constant bind group"),
                layout: &state.glyph_bind_group_layouts()[0],
                bindings: &[
                    wgpu::Binding {
                        binding: 0,
                        resource: wgpu::BindingResource::Sampler(state.diffuse_sampler()),
                    },
                    wgpu::Binding {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureViewArray(&texture_views[..]),
                    },
                ],
            });

        let per_draw_bind_group = state
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("glyph per-draw bind group"),
                layout: &state.glyph_bind_group_layouts()[1],
                bindings: &[wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(
                        state.glyph_uniform_buffer().buffer().slice(..),
                    ),
                }],
            });

        GlyphRenderer {
            textures,
            texture_views,
            const_bind_group,
            per_draw_bind_group,
        }
    }

    pub fn generate_uniforms(
        &self,
        commands: &[GlyphRendererCommand],
        display_width: u32,
        display_height: u32,
    ) -> Vec<GlyphUniforms> {
        let mut uniforms = Vec::new();
        for cmd in commands {
            match cmd {
                GlyphRendererCommand::Glyph {
                    glyph_id,
                    position,
                    anchor,
                } => {
                    let (screen_x, screen_y) = position.to_xy(display_width, display_height);
                    let (glyph_x, glyph_y) = anchor.to_xy(GLYPH_WIDTH as u32, GLYPH_HEIGHT as u32);
                    let x = screen_x - glyph_x;
                    let y = screen_y - glyph_y;

                    uniforms.push(GlyphUniforms {
                        transform: screen_space_vertex_transform(
                            display_width,
                            display_height,
                            GLYPH_WIDTH as u32,
                            GLYPH_HEIGHT as u32,
                            x,
                            y,
                        ),
                        layer: *glyph_id as u32,
                    });
                }
                GlyphRendererCommand::Text {
                    text,
                    position,
                    anchor,
                } => {
                    let (screen_x, screen_y) = position.to_xy(display_width, display_height);
                    let (glyph_x, glyph_y) =
                        anchor.to_xy((text.len() * GLYPH_WIDTH) as u32, GLYPH_HEIGHT as u32);
                    let x = screen_x - glyph_x;
                    let y = screen_y - glyph_y;

                    for (chr_id, chr) in text.as_str().chars().enumerate() {
                        let abs_x = x + (GLYPH_WIDTH * chr_id) as i32;

                        if abs_x >= display_width as i32 {
                            // don't render past the edge of the screen
                            break;
                        }

                        uniforms.push(GlyphUniforms {
                            transform: screen_space_vertex_transform(
                                display_width,
                                display_height,
                                GLYPH_WIDTH as u32,
                                GLYPH_HEIGHT as u32,
                                abs_x,
                                y,
                            ),
                            layer: chr as u32,
                        });
                    }
                }
            }
        }

        uniforms
    }

    pub fn record_draw<'a, 'b>(
        &'b self,
        state: &'b GraphicsState<'a>,
        pass: &mut wgpu::RenderPass<'b>,
        blocks: &[DynamicUniformBufferBlock<'a, GlyphUniforms>],
    ) where
        'a: 'b,
    {
        pass.set_pipeline(state.glyph_pipeline());
        pass.set_vertex_buffer(0, state.quad_vertex_buffer().slice(..));
        pass.set_bind_group(0, &self.const_bind_group, &[]);

        for block in blocks {
            pass.set_bind_group(1, &self.per_draw_bind_group, &[block.offset()]);
            pass.draw(0..6, 0..1);
        }
    }
}
