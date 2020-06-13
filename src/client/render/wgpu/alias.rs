use std::{borrow::Cow, cell::Cell, collections::HashMap, mem::size_of, ops::Range, rc::Rc};

use crate::{
    client::render::wgpu::{
        warp, BindGroupLayoutId, Camera, DynamicUniformBufferBlock, EntityUniforms, GraphicsState,
        LightmapData, TextureData, COLOR_ATTACHMENT_FORMAT, DEPTH_ATTACHMENT_FORMAT,
    },
    common::{
        math,
        mdl::{self, AliasModel},
        util::any_slice_as_bytes,
    },
};

use cgmath::{Deg, InnerSpace, Vector3};
use chrono::Duration;
use failure::Error;
use num::FromPrimitive;
use strum_macros::EnumIter;

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
        label: Some("brush per-texture chain bind group"),
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
    stride: size_of::<AliasVertex>() as u64,
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
    let alias_bind_group_layout_descriptors: Vec<wgpu::BindGroupLayoutDescriptor> =
        BIND_GROUP_LAYOUT_DESCRIPTORS.to_vec();

    debug!(
        "alias_bind_group_layout_descriptors = {:#?}",
        &alias_bind_group_layout_descriptors
    );

    let alias_bind_group_layouts: Vec<wgpu::BindGroupLayout> = alias_bind_group_layout_descriptors
        .iter()
        .map(|desc| device.create_bind_group_layout(desc))
        .collect();

    let alias_pipeline_layout = {
        let layouts: Vec<&wgpu::BindGroupLayout> = bind_group_layouts
            .into_iter()
            .map(|layout| layout)
            .chain(alias_bind_group_layouts.iter())
            .collect();
        let desc = wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &layouts,
        };
        device.create_pipeline_layout(&desc)
    };

    let mut compiler = shaderc::Compiler::new().unwrap();
    let alias_vertex_shader_spirv = compiler
        .compile_into_spirv(
            VERTEX_SHADER_GLSL,
            shaderc::ShaderKind::Vertex,
            "alias.vert",
            "main",
            None,
        )
        .unwrap();
    let alias_vertex_shader = device.create_shader_module(alias_vertex_shader_spirv.as_binary());
    let alias_fragment_shader_spirv = compiler
        .compile_into_spirv(
            FRAGMENT_SHADER_GLSL,
            shaderc::ShaderKind::Fragment,
            "alias.frag",
            "main",
            None,
        )
        .unwrap();
    let alias_fragment_shader =
        device.create_shader_module(alias_fragment_shader_spirv.as_binary());

    let alias_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        layout: &alias_pipeline_layout,
        vertex_stage: wgpu::ProgrammableStageDescriptor {
            module: &alias_vertex_shader,
            entry_point: "main",
        },
        fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
            module: &alias_fragment_shader,
            entry_point: "main",
        }),
        rasterization_state: Some(wgpu::RasterizationStateDescriptor {
            front_face: wgpu::FrontFace::Cw,
            cull_mode: wgpu::CullMode::Back,
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

    (alias_pipeline, alias_bind_group_layouts)
}

// these type aliases are here to aid readability of e.g. size_of::<Position>()
type Position = [f32; 3];
type DiffuseTexcoord = [f32; 2];

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct AliasVertex {
    position: Position,
    diffuse_texcoord: DiffuseTexcoord,
}

enum Keyframe {
    Static {
        vertex_range: Range<u32>,
    },
    Animated {
        vertex_ranges: Vec<Range<u32>>,
        total_duration: Duration,
        durations: Vec<Duration>,
    },
}

impl Keyframe {
    fn animate(&self, time: Duration) -> Range<u32> {
        match self {
            Keyframe::Static { vertex_range } => vertex_range.clone(),
            Keyframe::Animated {
                vertex_ranges,
                total_duration,
                durations,
            } => {
                let mut time_ms = time.num_milliseconds() % total_duration.num_milliseconds();

                for (frame_id, frame_duration) in durations.iter().enumerate() {
                    time_ms -= frame_duration.num_milliseconds();
                    if time_ms <= 0 {
                        return vertex_ranges[frame_id].clone();
                    }
                }

                unreachable!()
            }
        }
    }
}

enum Texture {
    Static {
        diffuse_texture: wgpu::Texture,
        diffuse_view: wgpu::TextureView,
        bind_group: wgpu::BindGroup,
    },
    Animated {
        diffuse_textures: Vec<wgpu::Texture>,
        diffuse_views: Vec<wgpu::TextureView>,
        bind_groups: Vec<wgpu::BindGroup>,
        total_duration: Duration,
        durations: Vec<Duration>,
    },
}

impl Texture {
    fn animate(&self, time: Duration) -> &wgpu::BindGroup {
        match self {
            Texture::Static { ref bind_group, .. } => bind_group,
            Texture::Animated {
                diffuse_textures,
                diffuse_views,
                bind_groups,
                total_duration,
                durations,
            } => {
                let mut time_ms = time.num_milliseconds() % total_duration.num_milliseconds();

                for (frame_id, frame_duration) in durations.iter().enumerate() {
                    time_ms -= frame_duration.num_milliseconds();
                    if time_ms <= 0 {
                        return &bind_groups[frame_id];
                    }
                }

                unreachable!()
            }
        }
    }
}

pub struct AliasRenderer {
    keyframes: Vec<Keyframe>,
    textures: Vec<Texture>,
    vertex_buffer: wgpu::Buffer,
}

impl AliasRenderer {
    pub fn new<'a>(
        state: Rc<GraphicsState<'a>>,
        alias_model: &AliasModel,
    ) -> Result<AliasRenderer, Error> {
        let mut vertices = Vec::new();
        let mut keyframes = Vec::new();

        let w = alias_model.texture_width();
        let h = alias_model.texture_height();
        for keyframe in alias_model.keyframes() {
            match *keyframe {
                mdl::Keyframe::Static(ref static_keyframe) => {
                    let vertex_start = vertices.len() as u32;
                    for polygon in alias_model.polygons() {
                        for index in polygon.indices() {
                            let pos = static_keyframe.vertices()[*index as usize];
                            let texcoord = &alias_model.texcoords()[*index as usize];

                            let s = if !polygon.faces_front() && texcoord.is_on_seam() {
                                (texcoord.s() + w / 2) as f32 + 0.5
                            } else {
                                texcoord.s() as f32 + 0.5
                            } / w as f32;

                            let t = (texcoord.t() as f32 + 0.5) / h as f32;
                            vertices.push(AliasVertex {
                                position: pos.into(),
                                diffuse_texcoord: [s, t],
                            });
                        }
                    }
                    let vertex_end = vertices.len() as u32;

                    keyframes.push(Keyframe::Static {
                        vertex_range: vertex_start..vertex_end,
                    });
                }

                mdl::Keyframe::Animated(ref kf) => {
                    let mut durations = Vec::new();
                    let mut vertex_ranges = Vec::new();

                    for frame in kf.frames() {
                        durations.push(frame.duration());

                        let vertex_start = vertices.len() as u32;
                        for polygon in alias_model.polygons() {
                            for index in polygon.indices().iter() {
                                let pos = frame.vertices()[*index as usize];
                                let texcoord = &alias_model.texcoords()[*index as usize];

                                let s = if !polygon.faces_front() && texcoord.is_on_seam() {
                                    (texcoord.s() + w / 2) as f32 + 0.5
                                } else {
                                    texcoord.s() as f32 + 0.5
                                } / w as f32;

                                let t = (texcoord.t() as f32 + 0.5) / h as f32;
                                vertices.push(AliasVertex {
                                    position: pos.into(),
                                    diffuse_texcoord: [s, t],
                                });
                            }
                        }
                        let vertex_end = vertices.len() as u32;
                        vertex_ranges.push(vertex_start..vertex_end);
                    }

                    let mut total_duration = durations.iter().fold(Duration::zero(), |s, d| s + *d);
                    keyframes.push(Keyframe::Animated {
                        vertex_ranges,
                        durations,
                        total_duration,
                    });
                }
            }
        }

        let vertex_buffer = state.device().create_buffer_with_data(
            unsafe { any_slice_as_bytes(vertices.as_slice()) },
            wgpu::BufferUsage::VERTEX,
        );

        let mut textures = Vec::new();
        for texture in alias_model.textures() {
            match *texture {
                mdl::Texture::Static(ref tex) => {
                    let (diffuse_data, fullbright_data) = state.palette.translate(tex.indices());
                    let diffuse_texture =
                        state.create_texture(None, w, h, &TextureData::Diffuse(diffuse_data));
                    let diffuse_view = diffuse_texture.create_default_view();
                    let bind_group = state
                        .device()
                        .create_bind_group(&wgpu::BindGroupDescriptor {
                            label: None,
                            layout: &state.alias_bind_group_layout(BindGroupLayoutId::PerTexture),
                            bindings: &[wgpu::Binding {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&diffuse_view),
                            }],
                        });
                    textures.push(Texture::Static {
                        diffuse_texture,
                        diffuse_view,
                        bind_group,
                    });
                }
                mdl::Texture::Animated(ref tex) => {
                    let mut total_duration = Duration::zero();
                    let mut durations = Vec::new();
                    let mut diffuse_textures = Vec::new();
                    let mut diffuse_views = Vec::new();
                    let mut bind_groups = Vec::new();

                    for frame in tex.frames() {
                        total_duration = total_duration + frame.duration();
                        durations.push(frame.duration());

                        let (diffuse_data, fullbright_data) =
                            state.palette.translate(frame.indices());
                        let diffuse_texture =
                            state.create_texture(None, w, h, &TextureData::Diffuse(diffuse_data));
                        let diffuse_view = diffuse_texture.create_default_view();
                        let bind_group =
                            state
                                .device()
                                .create_bind_group(&wgpu::BindGroupDescriptor {
                                    label: None,
                                    layout: &state
                                        .alias_bind_group_layout(BindGroupLayoutId::PerTexture),
                                    bindings: &[wgpu::Binding {
                                        binding: 0,
                                        resource: wgpu::BindingResource::TextureView(&diffuse_view),
                                    }],
                                });

                        diffuse_textures.push(diffuse_texture);
                        diffuse_views.push(diffuse_view);
                        bind_groups.push(bind_group);
                    }

                    textures.push(Texture::Animated {
                        diffuse_textures,
                        diffuse_views,
                        bind_groups,
                        total_duration,
                        durations,
                    });
                }
            }
        }

        Ok(AliasRenderer {
            keyframes,
            textures,
            vertex_buffer,
        })
    }

    pub fn record_draw<'a, 'b>(
        &'b self,
        state: &'b GraphicsState<'a>,
        pass: &mut wgpu::RenderPass<'b>,
        camera: &Camera,
        time: Duration,
        keyframe_id: usize,
        texture_id: usize,
    ) where
        'a: 'b,
    {
        pass.set_pipeline(state.alias_pipeline());
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

        let diffuse_view = self.textures[texture_id].animate(time);

        pass.set_bind_group(
            BindGroupLayoutId::PerTexture as u32,
            self.textures[texture_id].animate(time),
            &[],
        );
        pass.draw(self.keyframes[keyframe_id].animate(time), 0..1)
    }
}
