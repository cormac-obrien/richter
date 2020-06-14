use std::mem::size_of;

use crate::{
    client::render::wgpu::{
        warp, BindGroupLayoutId, GraphicsState, TextureData, COLOR_ATTACHMENT_FORMAT,
        DEPTH_ATTACHMENT_FORMAT,
    },
    common::sprite::{SpriteFrame, SpriteKind, SpriteModel, SpriteSubframe},
};

use chrono::Duration;

static VERTEX_SHADER_GLSL: &'static str = r#"
#version 450

layout(location = 0) in vec3 a_position;
layout(location = 1) in vec2 a_diffuse;

layout(location = 0) out vec2 f_diffuse;
layout(location = 1) out vec2 f_lightmap;
layout(location = 2) out uvec4 f_lightmap_anim;

layout(set = 0, binding = 0) uniform FrameUniforms {
    float light_anim_frames[64];
    vec4 camera_pos;
    float time;
} frame_uniforms;

layout(set = 1, binding = 0) uniform EntityUniforms {
    mat4 u_transform;
} entity_uniforms;

void main() {
    f_diffuse = a_diffuse;
    gl_Position = entity_uniforms.u_transform * vec4(-a_position.y, a_position.z, -a_position.x, 1.0);

}
"#;

static FRAGMENT_SHADER_GLSL: &'static str = r#"
#version 450

layout(location = 0) in vec2 f_diffuse;
layout(location = 1) in vec2 f_lightmap;

// set 0: per-frame
layout(set = 0, binding = 0) uniform FrameUniforms {
    float light_anim_frames[64];
    vec4 camera_pos;
    float time;
} frame_uniforms;

// set 1: per-entity
layout(set = 1, binding = 1) uniform sampler u_diffuse_sampler;

// set 2: per-texture chain
layout(set = 2, binding = 0) uniform texture2D u_diffuse_texture;

layout(location = 0) out vec4 color_attachment;

void main() {
    vec4 base_color = texture(sampler2D(u_diffuse_texture, u_diffuse_sampler), f_diffuse);
    color_attachment = base_color;
}
"#;

// NOTE: if any of the binding indices are changed, they must also be changed in
// the corresponding shaders and the BindGroupLayout generation functions.
// TODO: move diffuse sampler into its own group
const BIND_GROUP_LAYOUT_DESCRIPTORS: [wgpu::BindGroupLayoutDescriptor; 1] = [
    // group 2: updated per-texture
    wgpu::BindGroupLayoutDescriptor {
        label: Some("sprite per-texture chain bind group"),
        bindings: &[
            // diffuse texture, updated once per face
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStage::FRAGMENT,
                ty: wgpu::BindingType::SampledTexture {
                    dimension: wgpu::TextureViewDimension::D2,
                    component_type: wgpu::TextureComponentType::Float,
                    multisampled: false,
                },
            },
        ],
    },
];

// NOTE: if the vertex format is changed, this descriptor must also be changed accordingly.
const VERTEX_BUFFER_DESCRIPTOR: wgpu::VertexBufferDescriptor = wgpu::VertexBufferDescriptor {
    stride: size_of::<SpriteVertex>() as u64,
    step_mode: wgpu::InputStepMode::Vertex,
    attributes: &[
        // position
        wgpu::VertexAttributeDescriptor {
            offset: 0,
            format: wgpu::VertexFormat::Float3,
            shader_location: 0,
        },
        // diffuse texcoord
        wgpu::VertexAttributeDescriptor {
            offset: size_of::<Position>() as u64,
            format: wgpu::VertexFormat::Float2,
            shader_location: 1,
        },
    ],
};

pub fn create_render_pipeline<'a, I>(
    device: &wgpu::Device,
    bind_group_layouts: I,
) -> (wgpu::RenderPipeline, Vec<wgpu::BindGroupLayout>)
where
    I: IntoIterator<Item = &'a wgpu::BindGroupLayout>,
{
    let sprite_bind_group_layout_descriptors: Vec<wgpu::BindGroupLayoutDescriptor> =
        BIND_GROUP_LAYOUT_DESCRIPTORS.to_vec();

    debug!(
        "sprite_bind_group_layout_descriptors = {:#?}",
        &sprite_bind_group_layout_descriptors
    );

    let sprite_bind_group_layouts: Vec<wgpu::BindGroupLayout> =
        sprite_bind_group_layout_descriptors
            .iter()
            .map(|desc| device.create_bind_group_layout(desc))
            .collect();

    let sprite_pipeline_layout = {
        let layouts: Vec<&wgpu::BindGroupLayout> = bind_group_layouts
            .into_iter()
            .map(|layout| layout)
            .chain(sprite_bind_group_layouts.iter())
            .collect();
        let desc = wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &layouts,
        };
        device.create_pipeline_layout(&desc)
    };

    let mut compiler = shaderc::Compiler::new().unwrap();
    let sprite_vertex_shader_spirv = compiler
        .compile_into_spirv(
            VERTEX_SHADER_GLSL,
            shaderc::ShaderKind::Vertex,
            "sprite.vert",
            "main",
            None,
        )
        .unwrap();
    let sprite_vertex_shader = device.create_shader_module(sprite_vertex_shader_spirv.as_binary());
    let sprite_fragment_shader_spirv = compiler
        .compile_into_spirv(
            FRAGMENT_SHADER_GLSL,
            shaderc::ShaderKind::Fragment,
            "sprite.frag",
            "main",
            None,
        )
        .unwrap();
    let sprite_fragment_shader =
        device.create_shader_module(sprite_fragment_shader_spirv.as_binary());

    let sprite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        layout: &sprite_pipeline_layout,
        vertex_stage: wgpu::ProgrammableStageDescriptor {
            module: &sprite_vertex_shader,
            entry_point: "main",
        },
        fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
            module: &sprite_fragment_shader,
            entry_point: "main",
        }),
        rasterization_state: Some(wgpu::RasterizationStateDescriptor {
            front_face: wgpu::FrontFace::Cw,
            cull_mode: wgpu::CullMode::None,
            ..Default::default()
        }),
        primitive_topology: wgpu::PrimitiveTopology::TriangleList,
        color_states: &[wgpu::ColorStateDescriptor {
            format: COLOR_ATTACHMENT_FORMAT,
            alpha_blend: wgpu::BlendDescriptor::REPLACE,
            color_blend: wgpu::BlendDescriptor::REPLACE,
            write_mask: wgpu::ColorWrite::ALL,
        }],
        depth_stencil_state: Some(wgpu::DepthStencilStateDescriptor {
            format: DEPTH_ATTACHMENT_FORMAT,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil_front: wgpu::StencilStateFaceDescriptor::IGNORE,
            stencil_back: wgpu::StencilStateFaceDescriptor::IGNORE,
            stencil_read_mask: 0,
            stencil_write_mask: 0,
        }),
        vertex_state: wgpu::VertexStateDescriptor {
            index_format: wgpu::IndexFormat::Uint32,
            vertex_buffers: &[VERTEX_BUFFER_DESCRIPTOR],
        },
        sample_count: 1,
        sample_mask: !0,
        alpha_to_coverage_enabled: false,
    });

    (sprite_pipeline, sprite_bind_group_layouts)
}

// these type aliases are here to aid readability of e.g. size_of::<Position>()
type Position = [f32; 3];
type DiffuseTexcoord = [f32; 2];

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SpriteVertex {
    position: Position,
    diffuse_texcoord: DiffuseTexcoord,
}

pub const VERTICES: [SpriteVertex; 6] = [
    SpriteVertex {
        position: [0.0, 0.0, 0.0],
        diffuse_texcoord: [0.0, 1.0],
    },
    SpriteVertex {
        position: [0.0, 1.0, 0.0],
        diffuse_texcoord: [0.0, 0.0],
    },
    SpriteVertex {
        position: [1.0, 1.0, 0.0],
        diffuse_texcoord: [1.0, 0.0],
    },
    SpriteVertex {
        position: [0.0, 0.0, 0.0],
        diffuse_texcoord: [0.0, 1.0],
    },
    SpriteVertex {
        position: [1.0, 1.0, 0.0],
        diffuse_texcoord: [1.0, 0.0],
    },
    SpriteVertex {
        position: [1.0, 0.0, 0.0],
        diffuse_texcoord: [1.0, 1.0],
    },
];

enum Frame {
    Static {
        diffuse: wgpu::Texture,
        diffuse_view: wgpu::TextureView,
        bind_group: wgpu::BindGroup,
    },
    Animated {
        diffuses: Vec<wgpu::Texture>,
        diffuse_views: Vec<wgpu::TextureView>,
        bind_groups: Vec<wgpu::BindGroup>,
        total_duration: Duration,
        durations: Vec<Duration>,
    },
}

impl Frame {
    fn new(state: &GraphicsState, sframe: &SpriteFrame) -> Frame {
        fn convert_subframe(
            state: &GraphicsState,
            subframe: &SpriteSubframe,
        ) -> (wgpu::Texture, wgpu::TextureView, wgpu::BindGroup) {
            let (diffuse_data, fullbright_data) = state.palette.translate(subframe.indexed());
            let diffuse = state.create_texture(
                None,
                subframe.width(),
                subframe.height(),
                &TextureData::Diffuse(diffuse_data),
            );
            let diffuse_view = diffuse.create_default_view();
            let bind_group = state
                .device()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &state.sprite_bind_group_layout(BindGroupLayoutId::PerTexture),
                    bindings: &[wgpu::Binding {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&diffuse_view),
                    }],
                });
            (diffuse, diffuse_view, bind_group)
        }

        match sframe {
            SpriteFrame::Static { frame } => {
                let (diffuse, diffuse_view, bind_group) = convert_subframe(state, frame);

                Frame::Static {
                    diffuse,
                    diffuse_view,
                    bind_group,
                }
            }

            SpriteFrame::Animated {
                subframes,
                durations,
            } => {
                let mut diffuses = Vec::new();
                let mut diffuse_views = Vec::new();
                let mut bind_groups = Vec::new();

                let _ = subframes
                    .iter()
                    .map(|subframe| {
                        let (diffuse, diffuse_view, bind_group) = convert_subframe(state, subframe);
                        diffuses.push(diffuse);
                        diffuse_views.push(diffuse_view);
                        bind_groups.push(bind_group);
                    })
                    .count(); // count to consume the iterator

                let total_duration = durations.iter().fold(Duration::zero(), |init, d| init + *d);

                Frame::Animated {
                    diffuses,
                    diffuse_views,
                    bind_groups,
                    total_duration,
                    durations: durations.clone(),
                }
            }
        }
    }

    fn animate(&self, time: Duration) -> &wgpu::BindGroup {
        match self {
            Frame::Static { bind_group, .. } => &bind_group,
            Frame::Animated {
                bind_groups,
                total_duration,
                durations,
                ..
            } => {
                let mut time_ms = time.num_milliseconds() % total_duration.num_milliseconds();
                for (i, d) in durations.iter().enumerate() {
                    time_ms -= d.num_milliseconds();
                    if time_ms <= 0 {
                        return &bind_groups[i];
                    }
                }

                unreachable!()
            }
        }
    }
}

pub struct SpriteRenderer {
    kind: SpriteKind,
    frames: Vec<Frame>,
}

impl SpriteRenderer {
    pub fn new(state: &GraphicsState, sprite: &SpriteModel) -> SpriteRenderer {
        let frames = sprite
            .frames()
            .iter()
            .map(|f| Frame::new(state, f))
            .collect();

        SpriteRenderer {
            kind: sprite.kind(),
            frames,
        }
    }

    pub fn record_draw<'a, 'b>(
        &'b self,
        state: &'b GraphicsState<'a>,
        pass: &mut wgpu::RenderPass<'b>,
        frame_id: usize,
        time: Duration,
    ) {
        pass.set_pipeline(state.sprite_pipeline());
        pass.set_vertex_buffer(0, state.sprite_vertex_buffer().slice(..));
        pass.set_bind_group(
            BindGroupLayoutId::PerTexture as u32,
            self.frames[frame_id].animate(time),
            &[],
        );
        pass.draw(0..VERTICES.len() as u32, 0..1);
    }

    pub fn kind(&self) -> SpriteKind {
        self.kind
    }
}
