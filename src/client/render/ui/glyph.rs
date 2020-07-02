use std::mem::size_of;

use crate::client::render::{
    ui::{
        layout::{Anchor, ScreenPosition},
        quad::{QuadPipeline, QuadVertex},
        screen_space_vertex_scale, screen_space_vertex_translate,
    },
    GraphicsState, Pipeline, TextureData,
};

use cgmath::Vector2;

pub const GLYPH_WIDTH: usize = 8;
pub const GLYPH_HEIGHT: usize = 8;
const GLYPH_COLS: usize = 16;
const GLYPH_ROWS: usize = 16;
const GLYPH_COUNT: usize = GLYPH_ROWS * GLYPH_COLS;
const GLYPH_TEXTURE_WIDTH: usize = GLYPH_WIDTH * GLYPH_COLS;

/// The maximum number of glyphs that can be rendered at once.
pub const GLYPH_MAX_INSTANCES: usize = 65536;

lazy_static! {
    static ref BIND_GROUP_LAYOUT_DESCRIPTOR_BINDINGS: [Vec<wgpu::BindGroupLayoutEntry>; 1] = [
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
    ];

    static ref VERTEX_BUFFER_DESCRIPTOR_ATTRIBUTES: [Vec<wgpu::VertexAttributeDescriptor>; 2] = [
        wgpu::vertex_attr_array![
            0 => Float2, // a_position
            1 => Float2 // a_texcoord
        ].to_vec(),
        wgpu::vertex_attr_array![
            2 => Float2, // a_instance_position
            3 => Float2, // a_instance_scale
            4 => Uint // a_instance_layer
        ].to_vec(),
    ];
}

pub struct GlyphPipeline;

impl Pipeline for GlyphPipeline {
    fn name() -> &'static str {
        "glyph"
    }

    fn vertex_shader() -> &'static str {
        r#"
#version 450

// vertex rate
layout(location = 0) in vec2 a_position;
layout(location = 1) in vec2 a_texcoord;

// instance rate
layout(location = 2) in vec2 a_instance_position;
layout(location = 3) in vec2 a_instance_scale;
layout(location = 4) in uint a_instance_layer;

layout(location = 0) out vec2 f_texcoord;
layout(location = 1) out uint f_layer;

void main() {
    f_texcoord = a_texcoord;
    f_layer = a_instance_layer;
    gl_Position = vec4(a_instance_scale * a_position + a_instance_position, 0.0, 1.0);
}
"#
    }

    fn fragment_shader() -> &'static str {
        r#"
#version 450
#extension GL_EXT_nonuniform_qualifier : require

layout(location = 0) in vec2 f_texcoord;
layout(location = 1) flat in uint f_layer;

layout(location = 0) out vec4 output_attachment;

layout(set = 0, binding = 0) uniform sampler u_sampler;
layout(set = 0, binding = 1) uniform texture2D u_texture[256];

void main() {
    vec4 color = texture(sampler2D(u_texture[f_layer], u_sampler), f_texcoord);
    if (color.a == 0) {
        discard;
    } else {
        output_attachment = color;
    }
}
"#
    }

    fn bind_group_layout_descriptors() -> Vec<wgpu::BindGroupLayoutDescriptor<'static>> {
        vec![wgpu::BindGroupLayoutDescriptor {
            label: Some("glyph constant bind group"),
            bindings: &BIND_GROUP_LAYOUT_DESCRIPTOR_BINDINGS[0],
        }]
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
        vec![
            wgpu::VertexBufferDescriptor {
                stride: size_of::<QuadVertex>() as u64,
                step_mode: wgpu::InputStepMode::Vertex,
                attributes: &VERTEX_BUFFER_DESCRIPTOR_ATTRIBUTES[0],
            },
            wgpu::VertexBufferDescriptor {
                stride: size_of::<GlyphInstance>() as u64,
                step_mode: wgpu::InputStepMode::Instance,
                attributes: &VERTEX_BUFFER_DESCRIPTOR_ATTRIBUTES[1],
            },
        ]
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct GlyphInstance {
    pub position: Vector2<f32>,
    pub scale: Vector2<f32>,
    pub layer: u32,
}

pub enum GlyphRendererCommand {
    Glyph {
        glyph_id: u8,
        position: ScreenPosition,
        anchor: Anchor,
        scale: f32,
    },
    Text {
        text: String,
        position: ScreenPosition,
        anchor: Anchor,
        scale: f32,
    },
}

pub struct GlyphRenderer {
    #[allow(dead_code)]
    textures: Vec<wgpu::Texture>,
    #[allow(dead_code)]
    texture_views: Vec<wgpu::TextureView>,
    const_bind_group: wgpu::BindGroup,
}

impl GlyphRenderer {
    pub fn new(state: &GraphicsState) -> GlyphRenderer {
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

        GlyphRenderer {
            textures,
            texture_views,
            const_bind_group,
        }
    }

    pub fn generate_instances(
        &self,
        commands: &[GlyphRendererCommand],
        display_width: u32,
        display_height: u32,
    ) -> Vec<GlyphInstance> {
        let mut instances = Vec::new();
        for cmd in commands {
            match cmd {
                GlyphRendererCommand::Glyph {
                    glyph_id,
                    position,
                    anchor,
                    scale,
                } => {
                    let (screen_x, screen_y) =
                        position.to_xy(display_width, display_height, *scale);
                    let (glyph_x, glyph_y) = anchor.to_xy(
                        (GLYPH_WIDTH as f32 * scale) as u32,
                        (GLYPH_HEIGHT as f32 * scale) as u32,
                    );
                    let x = screen_x - glyph_x;
                    let y = screen_y - glyph_y;

                    instances.push(GlyphInstance {
                        position: screen_space_vertex_translate(
                            display_width,
                            display_height,
                            x,
                            y,
                        ),
                        scale: screen_space_vertex_scale(
                            display_width,
                            display_height,
                            (GLYPH_WIDTH as f32 * scale) as u32,
                            (GLYPH_HEIGHT as f32 * scale) as u32,
                        ),
                        layer: *glyph_id as u32,
                    });
                }
                GlyphRendererCommand::Text {
                    text,
                    position,
                    anchor,
                    scale,
                } => {
                    let (screen_x, screen_y) =
                        position.to_xy(display_width, display_height, *scale);
                    let (glyph_x, glyph_y) = anchor.to_xy(
                        ((text.chars().count() * GLYPH_WIDTH) as f32 * scale) as u32,
                        (GLYPH_HEIGHT as f32 * scale) as u32,
                    );
                    let x = screen_x - glyph_x;
                    let y = screen_y - glyph_y;

                    for (chr_id, chr) in text.as_str().chars().enumerate() {
                        let abs_x = x + ((GLYPH_WIDTH * chr_id) as f32 * scale) as i32;

                        if abs_x >= display_width as i32 {
                            // don't render past the edge of the screen
                            break;
                        }

                        instances.push(GlyphInstance {
                            position: screen_space_vertex_translate(
                                display_width,
                                display_height,
                                abs_x,
                                y,
                            ),
                            scale: screen_space_vertex_scale(
                                display_width,
                                display_height,
                                (GLYPH_WIDTH as f32 * scale) as u32,
                                (GLYPH_HEIGHT as f32 * scale) as u32,
                            ),
                            layer: chr as u32,
                        });
                    }
                }
            }
        }

        instances
    }

    pub fn record_draw<'a, 'b>(
        &'b self,
        state: &'b GraphicsState<'a>,
        pass: &mut wgpu::RenderPass<'b>,
        instance_count: u32,
    ) where
        'a: 'b,
    {
        pass.set_pipeline(state.glyph_pipeline());
        pass.set_vertex_buffer(0, state.quad_vertex_buffer().slice(..));
        pass.set_vertex_buffer(1, state.glyph_instance_buffer().slice(..));
        pass.set_bind_group(0, &self.const_bind_group, &[]);
        pass.draw(0..6, 0..instance_count);
    }
}
