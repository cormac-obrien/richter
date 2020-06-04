use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    mem::size_of,
    ops::Range,
    rc::Rc,
};

use crate::{
    client::render::wgpu::{
        DynamicUniformBufferBlock, EntityUniforms, GraphicsPackage, LightmapData, TextureData,
        COLOR_ATTACHMENT_FORMAT, DEPTH_ATTACHMENT_FORMAT,
    },
    common::{
        bsp::{self, BspData, BspFace, BspModel, BspTexInfo, BspTextureMipmap},
        math::collinear,
        util::{any_as_bytes, any_slice_as_bytes},
    },
};

use cgmath::{Deg, InnerSpace, Vector3};
use failure::Error;
use num::FromPrimitive;

static VERTEX_SHADER_GLSL: &'static str = r#"
#version 450

layout(location = 0) in vec3 a_position;
layout(location = 1) in vec2 a_diffuse;
layout(location = 2) in vec2 a_lightmap;
layout(location = 3) in uvec4 a_lightmap_anim;

layout(location = 0) out vec2 f_diffuse;
layout(location = 1) out vec2 f_lightmap;
layout(location = 2) out uvec4 f_lightmap_anim;

layout(set = 0, binding = 0) uniform FrameUniforms {
    float light_anim_frames[64];
    float time;
} frame_uniforms;

layout(set = 1, binding = 0) uniform EntityUniforms {
    mat4 u_transform;
} entity_uniforms;

void main() {
    f_diffuse = a_diffuse;
    f_lightmap = a_lightmap;
    f_lightmap_anim = a_lightmap_anim;
    gl_Position = entity_uniforms.u_transform * vec4(-a_position.y, a_position.z, -a_position.x, 1.0);
}
"#;

static FRAGMENT_SHADER_GLSL: &'static str = r#"
#version 450
#define LIGHTMAP_ANIM_END (255)

layout(location = 0) in vec2 f_diffuse; // also used for fullbright
layout(location = 1) in vec2 f_lightmap;
flat layout(location = 2) in uvec4 f_lightmap_anim;

// set 0: per-frame
layout(set = 0, binding = 0) uniform FrameUniforms {
    float light_anim_frames[64];
    float time;
} frame_uniforms;

// set 1: per-entity
layout(set = 1, binding = 1) uniform sampler u_diffuse_sampler; // also used for fullbright
layout(set = 1, binding = 2) uniform sampler u_lightmap_sampler;

// set 2: per-texture chain
layout(set = 2, binding = 0) uniform texture2D u_diffuse_texture;
layout(set = 2, binding = 1) uniform texture2D u_fullbright_texture;

// set 3: per-face
layout(set = 3, binding = 0) uniform texture2D u_lightmap_texture;

layout(location = 0) out vec4 color_attachment;

void main() {
    vec4 base_color = texture(sampler2D(u_diffuse_texture, u_diffuse_sampler), f_diffuse);
    if (texture(sampler2D(u_fullbright_texture, u_diffuse_sampler), f_diffuse).r == 1.0) {
        color_attachment = base_color;
        return;
    }

    vec4 lightmap = texture(sampler2D(u_lightmap_texture, u_lightmap_sampler), f_lightmap);
    vec4 lightmapped_color = vec4(base_color.rgb * lightmap.rrr, 1.0);

    float lightmap_factor = 0.0;
    for (int i = 0; i < f_lightmap_anim.length() && f_lightmap_anim[i] != LIGHTMAP_ANIM_END; i++) {
        lightmap_factor += frame_uniforms.light_anim_frames[f_lightmap_anim[i]];
    }

    color_attachment = lightmapped_color * lightmap_factor;
}
"#;

// NOTE: if any of the binding indices are changed, they must also be changed in
// the corresponding shaders and the BindGroupLayout generation functions.
// TODO: move diffuse sampler into its own group
const BIND_GROUP_LAYOUT_DESCRIPTORS: [wgpu::BindGroupLayoutDescriptor; 3] = [
    // group 0: updated per-frame
    // NOTE: we can't clone PER_FRAME_BIND_GROUP_LAYOUT_DESCRIPTOR here, but it
    // should always be the first bind group layout.

    // group 1: updated per-entity
    wgpu::BindGroupLayoutDescriptor {
        label: Some("brush per-entity bind group"),
        bindings: &[
            // transform matrix
            // TODO: move this to push constants once they're exposed in wgpu
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStage::VERTEX,
                ty: wgpu::BindingType::UniformBuffer { dynamic: true },
            },
            // diffuse and fullbright sampler
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStage::FRAGMENT,
                ty: wgpu::BindingType::Sampler { comparison: false },
            },
            // lightmap sampler
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStage::FRAGMENT,
                ty: wgpu::BindingType::Sampler { comparison: false },
            },
        ],
    },
    // group 2: updated per-texture chain
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
            // fullbright texture
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStage::FRAGMENT,
                ty: wgpu::BindingType::SampledTexture {
                    dimension: wgpu::TextureViewDimension::D2,
                    component_type: wgpu::TextureComponentType::Float,
                    multisampled: false,
                },
            },
        ],
    },
    // group 3: updated per-face
    wgpu::BindGroupLayoutDescriptor {
        label: Some("brush per-face bind group"),
        bindings: &[
            // lightmap texture
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
    stride: size_of::<BrushVertex>() as u64,
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
        // lightmap texcoord
        wgpu::VertexAttributeDescriptor {
            offset: (size_of::<Position>() + size_of::<DiffuseTexcoord>()) as u64,
            format: wgpu::VertexFormat::Float2,
            shader_location: 2,
        },
        // lightmap animation id(s)
        wgpu::VertexAttributeDescriptor {
            offset: (size_of::<Position>()
                + size_of::<DiffuseTexcoord>()
                + size_of::<LightmapTexcoord>()) as u64,
            format: wgpu::VertexFormat::Uchar4,
            shader_location: 3,
        },
    ],
};

pub fn create_render_pipeline(
    device: &wgpu::Device,
    per_frame_bind_group_layout: &wgpu::BindGroupLayout,
) -> (wgpu::RenderPipeline, Vec<wgpu::BindGroupLayout>) {
    let brush_bind_group_layout_descriptors: Vec<wgpu::BindGroupLayoutDescriptor> =
        BIND_GROUP_LAYOUT_DESCRIPTORS.to_vec();

    debug!(
        "brush_bind_group_layout_descriptors = {:#?}",
        &brush_bind_group_layout_descriptors
    );

    let brush_bind_group_layouts: Vec<wgpu::BindGroupLayout> = brush_bind_group_layout_descriptors
        .iter()
        .map(|desc| device.create_bind_group_layout(desc))
        .collect();

    let brush_pipeline_layout = {
        let layouts: Vec<&wgpu::BindGroupLayout> = std::iter::once(per_frame_bind_group_layout)
            .chain(brush_bind_group_layouts.iter())
            .collect();
        let desc = wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &layouts,
        };
        device.create_pipeline_layout(&desc)
    };

    let mut compiler = shaderc::Compiler::new().unwrap();
    let brush_vertex_shader_spirv = compiler
        .compile_into_spirv(
            VERTEX_SHADER_GLSL,
            shaderc::ShaderKind::Vertex,
            "brush.vert",
            "main",
            None,
        )
        .unwrap();
    let brush_vertex_shader = device.create_shader_module(brush_vertex_shader_spirv.as_binary());
    let brush_fragment_shader_spirv = compiler
        .compile_into_spirv(
            FRAGMENT_SHADER_GLSL,
            shaderc::ShaderKind::Fragment,
            "brush.frag",
            "main",
            None,
        )
        .unwrap();
    let brush_fragment_shader =
        device.create_shader_module(brush_fragment_shader_spirv.as_binary());

    let brush_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        layout: &brush_pipeline_layout,
        vertex_stage: wgpu::ProgrammableStageDescriptor {
            module: &brush_vertex_shader,
            entry_point: "main",
        },
        fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
            module: &brush_fragment_shader,
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

    (brush_pipeline, brush_bind_group_layouts)
}

fn calculate_lightmap_texcoords(
    position: Vector3<f32>,
    face: &BspFace,
    texinfo: &BspTexInfo,
) -> [f32; 2] {
    let mut s = texinfo.s_vector.dot(position) + texinfo.s_offset;
    s -= (face.texture_mins[0] as f32 / 16.0).floor() * 16.0;
    s += 0.5;
    s /= face.extents[0] as f32;

    let mut t = texinfo.t_vector.dot(position) + texinfo.t_offset;
    t -= (face.texture_mins[1] as f32 / 16.0).floor() * 16.0;
    t += 0.5;
    t /= face.extents[1] as f32;
    [s, t]
}

#[derive(Clone, Copy, Debug)]
pub enum BindGroupLayoutId {
    // starts at 1 because 0 is the per-frame group
    PerEntity = 1,
    PerTextureChain = 2,
    PerFace = 3,
}

// these type aliases are here to aid readability of e.g. size_of::<Position>()
type Position = [f32; 3];
type DiffuseTexcoord = [f32; 2];
type LightmapTexcoord = [f32; 2];
type LightmapAnim = [u8; 4];

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct BrushVertex {
    position: Position,
    diffuse_texcoord: DiffuseTexcoord,
    lightmap_texcoord: LightmapTexcoord,
    lightmap_anim: LightmapAnim,
}

#[derive(Debug)]
struct BrushFace {
    vertices: Range<u32>,
    texture_id: usize,
    lightmap_id: Option<usize>,
    light_styles: [u8; 4],
}

pub struct BrushRendererBuilder<'a> {
    gfx_pkg: Rc<GraphicsPackage<'a>>,
    bsp_data: Rc<BspData>,
    face_range: Range<usize>,

    per_texture_chain_bind_groups: Vec<wgpu::BindGroup>,
    per_face_bind_groups: Vec<wgpu::BindGroup>,

    vertices: Vec<BrushVertex>,
    faces: Vec<BrushFace>,
    texture_chains: HashMap<usize, Vec<usize>>,
    diffuses: Vec<wgpu::Texture>,
    diffuse_views: Vec<wgpu::TextureView>,
    fullbrights: Vec<wgpu::Texture>,
    fullbright_views: Vec<wgpu::TextureView>,
    lightmaps: Vec<wgpu::Texture>,
    lightmap_views: Vec<wgpu::TextureView>,
}

impl<'a> BrushRendererBuilder<'a> {
    pub fn new(bsp_model: &BspModel, gfx_pkg: Rc<GraphicsPackage<'a>>) -> BrushRendererBuilder<'a> {
        BrushRendererBuilder {
            gfx_pkg,
            bsp_data: bsp_model.bsp_data().clone(),
            face_range: bsp_model.face_id..bsp_model.face_id + bsp_model.face_count,
            per_texture_chain_bind_groups: Vec::new(),
            per_face_bind_groups: Vec::new(),
            vertices: Vec::new(),
            faces: Vec::new(),
            texture_chains: HashMap::new(),
            diffuses: Vec::new(),
            diffuse_views: Vec::new(),
            fullbrights: Vec::new(),
            fullbright_views: Vec::new(),
            lightmaps: Vec::new(),
            lightmap_views: Vec::new(),
        }
    }

    fn create_face<'b>(&'b mut self, face_id: usize) -> BrushFace {
        let face = &self.bsp_data.faces()[face_id];
        let face_vert_id = self.vertices.len();
        let texinfo = &self.bsp_data.texinfo()[face.texinfo_id];
        let tex = &self.bsp_data.textures()[texinfo.tex_id];

        let mut vert_iter = self.bsp_data.face_iter_vertices(face_id);

        // expand the vertices into a triangle list.
        // the vertices are guaranteed to be in valid triangle fan order (that's
        // how GLQuake renders them) so we expand from triangle fan to triangle
        // list order.
        //
        // v1 is the base vertex, so it remains constant.
        // v2 takes the previous value of v3.
        // v3 is the newest vertex.
        let v1 = vert_iter.next().unwrap();
        let mut v2 = vert_iter.next().unwrap();
        for v3 in vert_iter {
            let tri = &[v1, v2, v3];

            // skip collinear points
            if !collinear(tri) {
                for vert in tri.iter() {
                    self.vertices.push(BrushVertex {
                        position: (*vert).into(),
                        diffuse_texcoord: [
                            ((vert.dot(texinfo.s_vector) + texinfo.s_offset) / tex.width() as f32),
                            ((vert.dot(texinfo.t_vector) + texinfo.t_offset) / tex.height() as f32),
                        ],
                        lightmap_texcoord: calculate_lightmap_texcoords(
                            (*vert).into(),
                            face,
                            texinfo,
                        ),
                        lightmap_anim: face.light_styles,
                    });
                }
            }

            v2 = v3;
        }

        let lightmap_w = face.extents[0] / 16 + 1;
        let lightmap_h = face.extents[1] / 16 + 1;
        let lightmap_size = lightmap_w * lightmap_h;

        // build the lightmap
        let lightmap_id = if !texinfo.special {
            if let Some(ofs) = face.lightmap_id {
                let lightmap_data = TextureData::Lightmap(LightmapData {
                    lightmap: Cow::Borrowed(
                        &self.bsp_data.lightmaps()[ofs..ofs + lightmap_size as usize],
                    ),
                });

                let texture = self.gfx_pkg.create_texture(
                    None,
                    lightmap_w as u32,
                    lightmap_h as u32,
                    &lightmap_data,
                );

                let id = self.lightmaps.len();
                self.lightmaps.push(texture);
                self.lightmap_views
                    .push(self.lightmaps[id].create_default_view());
                Some(id)
            } else {
                None
            }
        } else {
            None
        };

        BrushFace {
            vertices: face_vert_id as u32..self.vertices.len() as u32,
            texture_id: texinfo.tex_id as usize,
            lightmap_id,
            light_styles: face.light_styles,
        }
    }

    fn create_per_entity_bind_group(&self) -> wgpu::BindGroup {
        let layout = &self
            .gfx_pkg
            .brush_bind_group_layout(BindGroupLayoutId::PerEntity);
        let ent_buf = self.gfx_pkg.entity_uniform_buffer();
        let desc = wgpu::BindGroupDescriptor {
            label: Some("brush per-entity bind group"),
            layout,
            bindings: &[
                wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(
                        ent_buf.buffer().slice(0..ent_buf.block_size().0),
                    ),
                },
                wgpu::Binding {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(self.gfx_pkg.diffuse_sampler()),
                },
                wgpu::Binding {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(self.gfx_pkg.lightmap_sampler()),
                },
            ],
        };
        self.gfx_pkg.device().create_bind_group(&desc)
    }

    fn create_per_texture_chain_bind_group(&self, texture_id: usize) -> wgpu::BindGroup {
        let layout = &self
            .gfx_pkg
            .brush_bind_group_layout(BindGroupLayoutId::PerTextureChain);
        let desc = wgpu::BindGroupDescriptor {
            label: Some("per-texture chain bind group"),
            layout,
            bindings: &[
                wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.diffuse_views[texture_id]),
                },
                wgpu::Binding {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &self.fullbright_views[texture_id],
                    ),
                },
            ],
        };
        self.gfx_pkg.device().create_bind_group(&desc)
    }

    fn create_per_face_bind_group(&self, face_id: usize) -> wgpu::BindGroup {
        let layout = &self
            .gfx_pkg
            .brush_bind_group_layout(BindGroupLayoutId::PerFace);
        let desc = wgpu::BindGroupDescriptor {
            label: Some("per-face bind group"),
            layout,
            bindings: &[wgpu::Binding {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(
                    match self.faces[face_id].lightmap_id {
                        Some(id) => &self.lightmap_views[id],
                        None => self.gfx_pkg.default_lightmap_view(),
                    },
                ),
            }],
        };
        self.gfx_pkg.device().create_bind_group(&desc)
    }

    pub fn build(mut self) -> Result<BrushRenderer<'a>, Error> {
        let per_entity_bind_group = self.create_per_entity_bind_group();

        // create the diffuse and fullbright textures
        for (tex_id, tex) in self.bsp_data.textures().iter().enumerate() {
            // let mut diffuses = Vec::new();
            // let mut fullbrights = Vec::new();
            // for i in 0..bsp::MIPLEVELS {
            //     let (diffuse_data, fullbright_data) = self
            //         .gfx_pkg
            //         .palette()
            //         .translate(tex.mipmap(BspTextureMipmap::from_usize(i).unwrap()));
            //     diffuses.push(diffuse_data);
            //     fullbrights.push(fullbright_data);
            // }

            let (diffuse_data, fullbright_data) = self
                .gfx_pkg
                .palette()
                .translate(tex.mipmap(BspTextureMipmap::from_usize(0).unwrap()));

            let (width, height) = tex.dimensions();
            let diffuse = self.gfx_pkg.create_texture(
                None,
                width,
                height,
                &TextureData::Diffuse(diffuse_data),
            );
            let fullbright = self.gfx_pkg.create_texture(
                None,
                width,
                height,
                &TextureData::Fullbright(fullbright_data),
            );

            self.diffuses.push(diffuse);
            self.diffuse_views
                .push(self.diffuses[self.diffuses.len() - 1].create_default_view());
            self.fullbrights.push(fullbright);
            self.fullbright_views
                .push(self.fullbrights[self.fullbrights.len() - 1].create_default_view());

            // generate texture bind group
            let per_texture_chain_bind_group = self.create_per_texture_chain_bind_group(tex_id);
            self.per_texture_chain_bind_groups
                .push(per_texture_chain_bind_group);
        }

        // generate faces, vertices and lightmaps
        // bsp_face_id is the id of the face in the bsp data
        // face_id is the new id of the face in the renderer
        for bsp_face_id in self.face_range.start..self.face_range.end {
            let face_id = self.faces.len();
            let face = self.create_face(bsp_face_id);
            self.faces.push(face);

            let face_tex_id = self.faces[face_id].texture_id;
            // update the corresponding texture chain
            self.texture_chains
                .entry(face_tex_id)
                .or_insert(Vec::new())
                .push(face_id);

            // generate face bind group
            let per_face_bind_group = self.create_per_face_bind_group(face_id);
            self.per_face_bind_groups.push(per_face_bind_group);
        }

        let vertex_buffer = self.gfx_pkg.device().create_buffer_with_data(
            unsafe { any_slice_as_bytes(self.vertices.as_slice()) },
            wgpu::BufferUsage::VERTEX,
        );

        Ok(BrushRenderer {
            gfx_pkg: self.gfx_pkg,
            bsp_data: self.bsp_data,
            vertex_buffer: vertex_buffer,
            per_entity_bind_group: per_entity_bind_group,
            per_texture_chain_bind_groups: self.per_texture_chain_bind_groups,
            per_face_bind_groups: self.per_face_bind_groups,
            texture_chains: self.texture_chains,
            faces: self.faces,
            diffuses: self.diffuses,
            diffuse_views: self.diffuse_views,
            fullbrights: self.fullbrights,
            fullbright_views: self.fullbright_views,
            lightmaps: self.lightmaps,
            lightmap_views: self.lightmap_views,
        })
    }
}

pub struct BrushRenderer<'a> {
    gfx_pkg: Rc<GraphicsPackage<'a>>,
    bsp_data: Rc<BspData>,

    vertex_buffer: wgpu::Buffer,
    per_entity_bind_group: wgpu::BindGroup,
    per_texture_chain_bind_groups: Vec<wgpu::BindGroup>,
    per_face_bind_groups: Vec<wgpu::BindGroup>,

    // faces are grouped by texture to reduce the number of texture rebinds
    // texture_chains maps texture ids to face ids
    texture_chains: HashMap<usize, Vec<usize>>,
    faces: Vec<BrushFace>,
    diffuses: Vec<wgpu::Texture>,
    diffuse_views: Vec<wgpu::TextureView>,
    fullbrights: Vec<wgpu::Texture>,
    fullbright_views: Vec<wgpu::TextureView>,
    lightmaps: Vec<wgpu::Texture>,
    lightmap_views: Vec<wgpu::TextureView>,
}

impl<'a> BrushRenderer<'a> {
    /// Record the draw commands for this brush model to the given `wgpu::RenderPass`.
    ///
    /// If `pvs` is `Some(set)`, then `set` contains the ids of leaves in the
    /// Potentially Visible Set, and only those leaves will be drawn.
    pub fn record_draw<'b>(
        &'b self,
        pass: &mut wgpu::RenderPass<'b>,
        entity_uniform_block: &DynamicUniformBufferBlock<'a, EntityUniforms>,
        pvs: Option<HashSet<usize>>,
    ) {
        pass.set_pipeline(self.gfx_pkg.brush_pipeline());
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

        pass.set_bind_group(
            BindGroupLayoutId::PerEntity as u32,
            &self.per_entity_bind_group,
            &[entity_uniform_block.offset()],
        );

        for (tex_id, face_ids) in self.texture_chains.iter() {
            pass.set_bind_group(
                BindGroupLayoutId::PerTextureChain as u32,
                &self.per_texture_chain_bind_groups[*tex_id],
                &[],
            );

            for face_id in face_ids.iter() {
                // if the face is not visible, don't draw it
                if let Some(ref set) = pvs {
                    if !set.contains(face_id) {
                        continue;
                    }
                }

                let face = &self.faces[*face_id];
                pass.set_bind_group(
                    BindGroupLayoutId::PerFace as u32,
                    &self.per_face_bind_groups[*face_id],
                    &[],
                );

                pass.draw(face.vertices.clone(), 0..1);
            }
        }
    }
}
