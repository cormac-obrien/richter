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

use std::{borrow::Cow, cell::Cell, collections::HashMap, mem::size_of, ops::Range, rc::Rc};

use crate::{
    client::render::{
        warp, world::BindGroupLayoutId, Camera, GraphicsState, LightmapData, Pipeline, TextureData,
        COLOR_ATTACHMENT_FORMAT, DEPTH_ATTACHMENT_FORMAT,
    },
    common::{
        bsp::{BspData, BspFace, BspLeaf, BspModel, BspTexInfo, BspTextureMipmap},
        math,
        util::any_slice_as_bytes,
    },
};

use cgmath::{InnerSpace, Vector3};
use failure::Error;
use num::FromPrimitive;
use strum_macros::EnumIter;

lazy_static! {
    static ref BIND_GROUP_LAYOUT_DESCRIPTOR_BINDINGS: [Vec<wgpu::BindGroupLayoutEntry>; 2] = [
        vec![
            // diffuse texture, updated once per face
            wgpu::BindGroupLayoutEntry::new(
                0,
                wgpu::ShaderStage::FRAGMENT,
                wgpu::BindingType::SampledTexture {
                    dimension: wgpu::TextureViewDimension::D2,
                    component_type: wgpu::TextureComponentType::Float,
                    multisampled: false,
                },
            ),
            // fullbright texture
            wgpu::BindGroupLayoutEntry::new(
                1,
                wgpu::ShaderStage::FRAGMENT,
                wgpu::BindingType::SampledTexture {
                    dimension: wgpu::TextureViewDimension::D2,
                    component_type: wgpu::TextureComponentType::Float,
                    multisampled: false,
                },
            ),
            // texture kind
            wgpu::BindGroupLayoutEntry::new(
                2,
                wgpu::ShaderStage::all(),
                wgpu::BindingType::UniformBuffer {
                    dynamic: true,
                    min_binding_size:
                        Some(
                            std::num::NonZeroU64::new(
                                size_of::<TextureUniforms>() as u64
                            )
                            .unwrap(),
                        ),
                },
            ),
        ],
        vec![
            // lightmap texture array
            wgpu::BindGroupLayoutEntry {
                count: Some(4),
                ..wgpu::BindGroupLayoutEntry::new(
                    0,
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
}

pub struct BrushPipeline;

impl Pipeline for BrushPipeline {
    fn name() -> &'static str {
        "brush"
    }

    fn vertex_shader() -> &'static str {
        r#"
#version 450

const uint TEXTURE_KIND_NORMAL = 0;
const uint TEXTURE_KIND_WARP = 1;
const uint TEXTURE_KIND_SKY = 2;

layout(location = 0) in vec3 a_position;
layout(location = 1) in vec2 a_diffuse;
layout(location = 2) in vec2 a_lightmap;
layout(location = 3) in uvec4 a_lightmap_anim;

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

layout(set = 2, binding = 2) uniform TextureUniforms {
    uint kind;
} texture_uniforms;

void main() {
    if (texture_uniforms.kind == TEXTURE_KIND_SKY) {
        vec3 dir = a_position - frame_uniforms.camera_pos.xyz;
        dir.z *= 3.0;

        // the coefficients here are magic taken from the Quake source
        float len = 6.0 * 63.0 / length(dir);
        dir = vec3(dir.xy * len, dir.z);
        f_diffuse = (mod(8.0 * frame_uniforms.time, 128.0) + dir.xy) / 128.0;
    } else {
        f_diffuse = a_diffuse;
    }

    f_lightmap = a_lightmap;
    f_lightmap_anim = a_lightmap_anim;
    gl_Position = entity_uniforms.u_transform * vec4(-a_position.y, a_position.z, -a_position.x, 1.0);

}
"#
    }

    fn fragment_shader() -> &'static str {
        r#"
#version 450
#define LIGHTMAP_ANIM_END (255)

const uint TEXTURE_KIND_NORMAL = 0;
const uint TEXTURE_KIND_WARP = 1;
const uint TEXTURE_KIND_SKY = 2;

const float WARP_AMPLITUDE = 0.15;
const float WARP_FREQUENCY = 0.25;
const float WARP_SCALE = 1.0;

layout(location = 0) in vec2 f_diffuse; // also used for fullbright
layout(location = 1) in vec2 f_lightmap;
flat layout(location = 2) in uvec4 f_lightmap_anim;

// set 0: per-frame
layout(set = 0, binding = 0) uniform FrameUniforms {
    uint light_anim_frames[64]; // range [0, 550]
    vec4 camera_pos;
    float time;
    bool r_lightmap;
} frame_uniforms;

// set 1: per-entity
layout(set = 1, binding = 1) uniform sampler u_diffuse_sampler; // also used for fullbright
layout(set = 1, binding = 2) uniform sampler u_lightmap_sampler;

// set 2: per-texture
layout(set = 2, binding = 0) uniform texture2D u_diffuse_texture;
layout(set = 2, binding = 1) uniform texture2D u_fullbright_texture;
layout(set = 2, binding = 2) uniform TextureUniforms {
    uint kind;
} texture_uniforms;

// set 3: per-face
layout(set = 3, binding = 0) uniform texture2D u_lightmap_texture[4];

layout(location = 0) out vec4 color_attachment;

// FIXME
vec4 blend_light(vec4 color) {
    float light = 0.0;
    for (int i = 0; i < 4 && f_lightmap_anim[i] != LIGHTMAP_ANIM_END; i++) {
        uint umap = uint(texture(
            sampler2D(u_lightmap_texture[i], u_lightmap_sampler),
            f_lightmap
        ).r * 255.0) * 2;
        uint ustyle = frame_uniforms.light_anim_frames[f_lightmap_anim[i]];
        uint ulight = umap * ustyle;
        light += float(min(ulight >> 8, 255)) / 256.0;
    }

    if (frame_uniforms.r_lightmap) {
        return vec4(light.rrr, 1.0);
    } else {
        return vec4(color.rgb * light.rrr, 1.0);
    }
}

void main() {
    switch (texture_uniforms.kind) {
        case TEXTURE_KIND_NORMAL:
            vec4 base_color = texture(sampler2D(u_diffuse_texture, u_diffuse_sampler), f_diffuse);
            float fullbright = texture(sampler2D(u_fullbright_texture, u_diffuse_sampler), f_diffuse).r;

            if (fullbright != 0.0) {
                color_attachment = base_color;
            } else {
                color_attachment = blend_light(base_color);
            }
            break;

        case TEXTURE_KIND_WARP:
            // note the texcoord transpose here
            vec2 wave1 = 3.14159265359 * (WARP_SCALE * f_diffuse.ts + WARP_FREQUENCY * frame_uniforms.time);
            vec2 warp_texcoord = f_diffuse.st + WARP_AMPLITUDE * vec2(sin(wave1.s), sin(wave1.t));
            color_attachment = texture(sampler2D(u_diffuse_texture, u_diffuse_sampler), warp_texcoord);
            break;

        case TEXTURE_KIND_SKY:
            vec2 base = mod(f_diffuse + frame_uniforms.time, 1.0);
            vec2 cloud_texcoord = vec2(base.s * 0.5, base.t);
            vec2 sky_texcoord = vec2(base.s * 0.5 + 0.5, base.t);

            vec4 sky_color = texture(sampler2D(u_diffuse_texture, u_diffuse_sampler), sky_texcoord);
            vec4 cloud_color = texture(sampler2D(u_diffuse_texture, u_diffuse_sampler), cloud_texcoord);

            // 0.0 if black, 1.0 otherwise
            // float cloud_factor = ceil(max(max(cloud_color.r, cloud_color.g), cloud_color.b));
            float cloud_factor;
            if (cloud_color.r + cloud_color.g + cloud_color.b == 0.0) {
                cloud_factor = 0.0;
            } else {
                cloud_factor = 1.0;
            }
            color_attachment = mix(sky_color, cloud_color, cloud_factor);
            break;

        // not possible
        default:
            break;
    }
}
"#
    }

    // NOTE: if any of the binding indices are changed, they must also be changed in
    // the corresponding shaders and the BindGroupLayout generation functions.
    fn bind_group_layout_descriptors() -> Vec<wgpu::BindGroupLayoutDescriptor<'static>> {
        vec![
            // group 2: updated per-texture
            wgpu::BindGroupLayoutDescriptor {
                label: Some("brush per-texture bind group"),
                bindings: &BIND_GROUP_LAYOUT_DESCRIPTOR_BINDINGS[0],
            },
            // group 3: updated per-face
            wgpu::BindGroupLayoutDescriptor {
                label: Some("brush per-face bind group"),
                bindings: &BIND_GROUP_LAYOUT_DESCRIPTOR_BINDINGS[1],
            },
        ]
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
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil_front: wgpu::StencilStateFaceDescriptor::IGNORE,
            stencil_back: wgpu::StencilStateFaceDescriptor::IGNORE,
            stencil_read_mask: 0,
            stencil_write_mask: 0,
        })
    }

    // NOTE: if the vertex format is changed, this descriptor must also be changed accordingly.
    fn vertex_buffer_descriptors() -> Vec<wgpu::VertexBufferDescriptor<'static>> {
        vec![wgpu::VertexBufferDescriptor {
            stride: size_of::<BrushVertex>() as u64,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![
                // position
                0 => Float3,
                // diffuse texcoord
                1 => Float2,
                // lightmap texcoord
                2 => Float2,
                // lightmap animation ids
                3 => Uchar4,
            ],
        }]
    }
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

#[repr(u32)]
#[derive(EnumIter, Clone, Copy, Debug)]
pub enum TextureKind {
    Normal = 0,
    Warp = 1,
    Sky = 2,
}

#[repr(C, align(256))]
#[derive(Clone, Copy, Debug)]
pub struct TextureUniforms {
    pub kind: TextureKind,
}

pub struct BrushTexture {
    diffuse: wgpu::Texture,
    fullbright: wgpu::Texture,
    diffuse_view: wgpu::TextureView,
    fullbright_view: wgpu::TextureView,
    kind: TextureKind,
}

#[derive(Debug)]
struct BrushFace {
    vertices: Range<u32>,
    texture_id: usize,

    lightmap_ids: Vec<usize>,
    light_styles: [u8; 4],

    /// Indicates whether the face should be drawn this frame.
    ///
    /// This is set to false by default, and will be set to true if the model is
    /// a worldmodel and the containing leaf is in the PVS. If the model is not
    /// a worldmodel, this flag is ignored.
    draw_flag: Cell<bool>,
}

struct BrushLeaf {
    facelist_ids: Range<usize>,
}

impl<B> std::convert::From<B> for BrushLeaf
where
    B: std::borrow::Borrow<BspLeaf>,
{
    fn from(bsp_leaf: B) -> Self {
        let bsp_leaf = bsp_leaf.borrow();
        BrushLeaf {
            facelist_ids: bsp_leaf.facelist_id..bsp_leaf.facelist_id + bsp_leaf.facelist_count,
        }
    }
}

pub struct BrushRendererBuilder {
    bsp_data: Rc<BspData>,
    face_range: Range<usize>,

    leaves: Option<Vec<BrushLeaf>>,

    per_texture_bind_groups: Vec<wgpu::BindGroup>,
    per_face_bind_groups: Vec<wgpu::BindGroup>,

    vertices: Vec<BrushVertex>,
    faces: Vec<BrushFace>,
    texture_chains: HashMap<usize, Vec<usize>>,
    textures: Vec<BrushTexture>,
    lightmaps: Vec<wgpu::Texture>,
    //lightmap_views: Vec<wgpu::TextureView>,
}

impl BrushRendererBuilder {
    pub fn new(bsp_model: &BspModel, worldmodel: bool) -> BrushRendererBuilder {
        BrushRendererBuilder {
            bsp_data: bsp_model.bsp_data().clone(),
            face_range: bsp_model.face_id..bsp_model.face_id + bsp_model.face_count,
            leaves: if worldmodel {
                Some(
                    bsp_model
                        .iter_leaves()
                        .map(|leaf| BrushLeaf::from(leaf))
                        .collect(),
                )
            } else {
                None
            },
            per_texture_bind_groups: Vec::new(),
            per_face_bind_groups: Vec::new(),
            vertices: Vec::new(),
            faces: Vec::new(),
            texture_chains: HashMap::new(),
            textures: Vec::new(),
            lightmaps: Vec::new(),
            //lightmap_views: Vec::new(),
        }
    }

    fn create_face<'a, 'b>(
        &'b mut self,
        state: &'b GraphicsState<'a>,
        face_id: usize,
    ) -> BrushFace {
        let face = &self.bsp_data.faces()[face_id];
        let face_vert_id = self.vertices.len();
        let texinfo = &self.bsp_data.texinfo()[face.texinfo_id];
        let tex = &self.bsp_data.textures()[texinfo.tex_id];

        if tex.name().starts_with("*") {
            // tessellate the surface so we can do texcoord warping
            let verts = warp::subdivide(math::remove_collinear(
                self.bsp_data.face_iter_vertices(face_id).collect(),
            ));
            for vert in verts.into_iter() {
                self.vertices.push(BrushVertex {
                    position: vert.into(),
                    diffuse_texcoord: [
                        ((vert.dot(texinfo.s_vector) + texinfo.s_offset) / tex.width() as f32),
                        ((vert.dot(texinfo.t_vector) + texinfo.t_offset) / tex.height() as f32),
                    ],
                    lightmap_texcoord: calculate_lightmap_texcoords(vert.into(), face, texinfo),
                    lightmap_anim: face.light_styles,
                })
            }
        } else {
            // expand the vertices into a triangle list.
            // the vertices are guaranteed to be in valid triangle fan order (that's
            // how GLQuake renders them) so we expand from triangle fan to triangle
            // list order.
            //
            // v1 is the base vertex, so it remains constant.
            // v2 takes the previous value of v3.
            // v3 is the newest vertex.
            let mut vert_iter =
                math::remove_collinear(self.bsp_data.face_iter_vertices(face_id).collect())
                    .into_iter();

            let v1 = vert_iter.next().unwrap();
            let mut v2 = vert_iter.next().unwrap();
            for v3 in vert_iter {
                let tri = &[v1, v2, v3];

                // skip collinear points
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

                v2 = v3;
            }
        }

        // build the lightmaps
        let lightmaps = if !texinfo.special {
            self.bsp_data.face_lightmaps(face_id)
        } else {
            Vec::new()
        };

        let mut lightmap_ids = Vec::new();
        for lightmap in lightmaps {
            let lightmap_data = TextureData::Lightmap(LightmapData {
                lightmap: Cow::Borrowed(lightmap.data()),
            });

            let texture =
                state.create_texture(None, lightmap.width(), lightmap.height(), &lightmap_data);

            let id = self.lightmaps.len();
            self.lightmaps.push(texture);
            //self.lightmap_views
            //.push(self.lightmaps[id].create_default_view());
            lightmap_ids.push(id);
        }

        BrushFace {
            vertices: face_vert_id as u32..self.vertices.len() as u32,
            texture_id: texinfo.tex_id as usize,
            lightmap_ids,
            light_styles: face.light_styles,
            draw_flag: Cell::new(true),
        }
    }

    fn create_per_texture_bind_group<'a>(
        &self,
        state: &GraphicsState<'a>,
        texture_id: usize,
    ) -> wgpu::BindGroup {
        let layout = &state.brush_bind_group_layout(BindGroupLayoutId::PerTexture);
        let tex = &self.textures[texture_id];
        let tex_unif_buf = state.brush_texture_uniform_buffer();
        let desc = wgpu::BindGroupDescriptor {
            label: Some("per-texture bind group"),
            layout,
            bindings: &[
                wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&tex.diffuse_view),
                },
                wgpu::Binding {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&tex.fullbright_view),
                },
                wgpu::Binding {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(tex_unif_buf.buffer().slice(..)),
                },
            ],
        };
        state.device().create_bind_group(&desc)
    }

    fn create_per_face_bind_group<'a>(
        &self,
        state: &GraphicsState<'a>,
        face_id: usize,
    ) -> wgpu::BindGroup {
        let mut lightmap_views: Vec<_> = self.faces[face_id]
            .lightmap_ids
            .iter()
            .map(|id| self.lightmaps[*id].create_default_view())
            .collect();
        lightmap_views.resize_with(4, || state.default_lightmap().create_default_view());
        let layout = &state.brush_bind_group_layout(BindGroupLayoutId::PerFace);
        let desc = wgpu::BindGroupDescriptor {
            label: Some("per-face bind group"),
            layout,
            bindings: &[wgpu::Binding {
                binding: 0,
                resource: wgpu::BindingResource::TextureViewArray(&lightmap_views[..]),
            }],
        };
        state.device().create_bind_group(&desc)
    }

    pub fn build<'a>(mut self, state: &GraphicsState<'a>) -> Result<BrushRenderer, Error> {
        // create the diffuse and fullbright textures
        for (tex_id, tex) in self.bsp_data.textures().iter().enumerate() {
            // let mut diffuses = Vec::new();
            // let mut fullbrights = Vec::new();
            // for i in 0..bsp::MIPLEVELS {
            //     let (diffuse_data, fullbright_data) = self
            //         .state
            //         .palette()
            //         .translate(tex.mipmap(BspTextureMipmap::from_usize(i).unwrap()));
            //     diffuses.push(diffuse_data);
            //     fullbrights.push(fullbright_data);
            // }

            let (diffuse_data, fullbright_data) = state
                .palette()
                .translate(tex.mipmap(BspTextureMipmap::from_usize(0).unwrap()));

            let (width, height) = tex.dimensions();
            let diffuse =
                state.create_texture(None, width, height, &TextureData::Diffuse(diffuse_data));
            let fullbright = state.create_texture(
                None,
                width,
                height,
                &TextureData::Fullbright(fullbright_data),
            );

            let diffuse_view = diffuse.create_default_view();
            let fullbright_view = fullbright.create_default_view();

            let kind = if tex.name().starts_with("sky") {
                debug!("sky texture");
                TextureKind::Sky
            } else if tex.name().starts_with("*") {
                debug!("warp texture");
                TextureKind::Warp
            } else {
                TextureKind::Normal
            };

            let texture = BrushTexture {
                diffuse,
                fullbright,
                diffuse_view,
                fullbright_view,
                kind,
            };

            self.textures.push(texture);

            // generate texture bind group
            let per_texture_bind_group = self.create_per_texture_bind_group(state, tex_id);
            self.per_texture_bind_groups.push(per_texture_bind_group);
        }

        // generate faces, vertices and lightmaps
        // bsp_face_id is the id of the face in the bsp data
        // face_id is the new id of the face in the renderer
        for bsp_face_id in self.face_range.start..self.face_range.end {
            let face_id = self.faces.len();
            let face = self.create_face(state, bsp_face_id);
            self.faces.push(face);

            let face_tex_id = self.faces[face_id].texture_id;
            // update the corresponding texture chain
            self.texture_chains
                .entry(face_tex_id)
                .or_insert(Vec::new())
                .push(face_id);

            // generate face bind group
            let per_face_bind_group = self.create_per_face_bind_group(state, face_id);
            self.per_face_bind_groups.push(per_face_bind_group);
        }

        let vertex_buffer = state.device().create_buffer_with_data(
            unsafe { any_slice_as_bytes(self.vertices.as_slice()) },
            wgpu::BufferUsage::VERTEX,
        );

        Ok(BrushRenderer {
            bsp_data: self.bsp_data,
            vertex_buffer,
            leaves: self.leaves,
            per_texture_bind_groups: self.per_texture_bind_groups,
            per_face_bind_groups: self.per_face_bind_groups,
            texture_chains: self.texture_chains,
            faces: self.faces,
            textures: self.textures,
            lightmaps: self.lightmaps,
            //lightmap_views: self.lightmap_views,
        })
    }
}

pub struct BrushRenderer {
    bsp_data: Rc<BspData>,

    leaves: Option<Vec<BrushLeaf>>,

    vertex_buffer: wgpu::Buffer,
    per_texture_bind_groups: Vec<wgpu::BindGroup>,
    per_face_bind_groups: Vec<wgpu::BindGroup>,

    // faces are grouped by texture to reduce the number of texture rebinds
    // texture_chains maps texture ids to face ids
    texture_chains: HashMap<usize, Vec<usize>>,
    faces: Vec<BrushFace>,
    textures: Vec<BrushTexture>,
    lightmaps: Vec<wgpu::Texture>,
    //lightmap_views: Vec<wgpu::TextureView>,
}

impl BrushRenderer {
    /// Record the draw commands for this brush model to the given `wgpu::RenderPass`.
    pub fn record_draw<'a, 'b>(
        &'b self,
        state: &'b GraphicsState<'a>,
        pass: &mut wgpu::RenderPass<'b>,
        camera: &Camera,
    ) {
        let _guard = flame::start_guard("BrushRenderer::record_draw");
        pass.set_pipeline(state.brush_pipeline());
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

        // if this is a worldmodel, mark faces to be drawn
        if let Some(ref leaves) = self.leaves {
            let pvs = self
                .bsp_data
                .get_pvs(self.bsp_data.find_leaf(camera.origin), leaves.len());
            for leaf_id in pvs {
                for facelist_id in leaves[leaf_id].facelist_ids.clone() {
                    self.faces[self.bsp_data.facelist()[facelist_id]]
                        .draw_flag
                        .set(true);
                }
            }
        }

        for (tex_id, face_ids) in self.texture_chains.iter() {
            let _tex_guard = flame::start_guard("texture chain");
            pass.set_bind_group(
                BindGroupLayoutId::PerTexture as u32,
                &self.per_texture_bind_groups[*tex_id],
                &[state
                    .brush_texture_uniform_block(self.textures[*tex_id].kind)
                    .offset()],
            );

            for face_id in face_ids.iter() {
                let face = &self.faces[*face_id];

                // only skip the face if we have visibility data but it's not marked
                if self.leaves.is_some() && !face.draw_flag.replace(false) {
                    continue;
                }

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
