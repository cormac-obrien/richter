use std::{mem::size_of, ops::Range};

use crate::{
    client::render::{
        world::{BindGroupLayoutId, WorldPipelineBase},
        GraphicsState, Pipeline, TextureData,
    },
    common::{
        mdl::{self, AliasModel},
        util::any_slice_as_bytes,
    },
};

use cgmath::{InnerSpace as _, Matrix4, Vector3, Zero as _};
use chrono::Duration;
use failure::Error;

lazy_static! {
    static ref BIND_GROUP_LAYOUT_DESCRIPTOR_BINDINGS: [Vec<wgpu::BindGroupLayoutEntry>; 1] = [
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
        ]
    ];
}

pub struct AliasPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layouts: Vec<wgpu::BindGroupLayout>,
}

impl AliasPipeline {
    pub fn new(
        device: &wgpu::Device,
        compiler: &mut shaderc::Compiler,
        world_bind_group_layouts: &[wgpu::BindGroupLayout],
        sample_count: u32,
    ) -> AliasPipeline {
        let (pipeline, bind_group_layouts) =
            AliasPipeline::create(device, compiler, world_bind_group_layouts, sample_count);

        AliasPipeline {
            pipeline,
            bind_group_layouts,
        }
    }

    pub fn rebuild(
        &mut self,
        device: &wgpu::Device,
        compiler: &mut shaderc::Compiler,
        world_bind_group_layouts: &[wgpu::BindGroupLayout],
        sample_count: u32,
    ) {
        let layout_refs: Vec<_> = world_bind_group_layouts
            .iter()
            .chain(self.bind_group_layouts.iter())
            .collect();
        self.pipeline = AliasPipeline::recreate(device, compiler, &layout_refs, sample_count);
    }

    pub fn pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }

    pub fn bind_group_layouts(&self) -> &[wgpu::BindGroupLayout] {
        &self.bind_group_layouts
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct VertexPushConstants {
    pub transform: Matrix4<f32>,
    pub model_view: Matrix4<f32>,
}

impl Pipeline for AliasPipeline {
    type VertexPushConstants = VertexPushConstants;
    type SharedPushConstants = ();
    type FragmentPushConstants = ();

    fn name() -> &'static str {
        "alias"
    }

    fn vertex_shader() -> &'static str {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/shaders/alias.vert"))
    }

    fn fragment_shader() -> &'static str {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/shaders/alias.frag"))
    }

    fn bind_group_layout_descriptors() -> Vec<wgpu::BindGroupLayoutDescriptor<'static>> {
        vec![
            // group 2: updated per-texture
            wgpu::BindGroupLayoutDescriptor {
                label: Some("brush per-texture chain bind group"),
                entries: &BIND_GROUP_LAYOUT_DESCRIPTOR_BINDINGS[0],
            },
        ]
    }

    fn rasterization_state_descriptor() -> Option<wgpu::RasterizationStateDescriptor> {
        WorldPipelineBase::rasterization_state_descriptor()
    }

    fn primitive_topology() -> wgpu::PrimitiveTopology {
        wgpu::PrimitiveTopology::TriangleList
    }

    fn color_state_descriptors() -> Vec<wgpu::ColorStateDescriptor> {
        WorldPipelineBase::color_state_descriptors()
    }

    fn depth_stencil_state_descriptor() -> Option<wgpu::DepthStencilStateDescriptor> {
        WorldPipelineBase::depth_stencil_state_descriptor()
    }

    // NOTE: if the vertex format is changed, this descriptor must also be changed accordingly.
    fn vertex_buffer_descriptors() -> Vec<wgpu::VertexBufferDescriptor<'static>> {
        vec![wgpu::VertexBufferDescriptor {
            stride: size_of::<AliasVertex>() as u64,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![
                // frame 0 position
                0 => Float3,
                // frame 1 position
                // 1 => Float3,
                // normal
                2 => Float3,
                // texcoord
                3 => Float2,
            ],
        }]
    }
}

// these type aliases are here to aid readability of e.g. size_of::<Position>()
type Position = [f32; 3];
type Normal = [f32; 3];
type DiffuseTexcoord = [f32; 2];

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct AliasVertex {
    position: Position,
    normal: Normal,
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
    pub fn new(state: &GraphicsState, alias_model: &AliasModel) -> Result<AliasRenderer, Error> {
        let mut vertices = Vec::new();
        let mut keyframes = Vec::new();

        let w = alias_model.texture_width();
        let h = alias_model.texture_height();
        for keyframe in alias_model.keyframes() {
            match *keyframe {
                mdl::Keyframe::Static(ref static_keyframe) => {
                    let vertex_start = vertices.len() as u32;
                    for polygon in alias_model.polygons() {
                        let mut tri = [Vector3::zero(); 3];
                        let mut texcoords = [[0.0; 2]; 3];
                        for (i, index) in polygon.indices().iter().enumerate() {
                            tri[i] = static_keyframe.vertices()[*index as usize].into();

                            let texcoord = &alias_model.texcoords()[*index as usize];
                            let s = if !polygon.faces_front() && texcoord.is_on_seam() {
                                (texcoord.s() + w / 2) as f32 + 0.5
                            } else {
                                texcoord.s() as f32 + 0.5
                            } / w as f32;
                            let t = (texcoord.t() as f32 + 0.5) / h as f32;
                            texcoords[i] = [s, t];
                        }

                        let normal = (tri[0] - tri[1]).cross(tri[2] - tri[1]).normalize();

                        for i in 0..3 {
                            vertices.push(AliasVertex {
                                position: tri[i].into(),
                                normal: normal.into(),
                                diffuse_texcoord: texcoords[i],
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
                            let mut tri = [Vector3::zero(); 3];
                            let mut texcoords = [[0.0; 2]; 3];
                            for (i, index) in polygon.indices().iter().enumerate() {
                                tri[i] = frame.vertices()[*index as usize].into();

                                let texcoord = &alias_model.texcoords()[*index as usize];
                                let s = if !polygon.faces_front() && texcoord.is_on_seam() {
                                    (texcoord.s() + w / 2) as f32 + 0.5
                                } else {
                                    texcoord.s() as f32 + 0.5
                                } / w as f32;
                                let t = (texcoord.t() as f32 + 0.5) / h as f32;
                                texcoords[i] = [s, t];
                            }

                            let normal = (tri[0] - tri[1]).cross(tri[2] - tri[1]).normalize();

                            for i in 0..3 {
                                vertices.push(AliasVertex {
                                    position: tri[i].into(),
                                    normal: normal.into(),
                                    diffuse_texcoord: texcoords[i],
                                });
                            }
                        }
                        let vertex_end = vertices.len() as u32;
                        vertex_ranges.push(vertex_start..vertex_end);
                    }

                    let total_duration = durations.iter().fold(Duration::zero(), |s, d| s + *d);
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
                    let (diffuse_data, _fullbright_data) = state.palette.translate(tex.indices());
                    let diffuse_texture =
                        state.create_texture(None, w, h, &TextureData::Diffuse(diffuse_data));
                    let diffuse_view = diffuse_texture.create_default_view();
                    let bind_group = state
                        .device()
                        .create_bind_group(&wgpu::BindGroupDescriptor {
                            label: None,
                            // TODO: per-pipeline bind group layout ids
                            layout: &state.alias_pipeline().bind_group_layouts()
                                [BindGroupLayoutId::PerTexture as usize - 2],
                            entries: &[wgpu::BindGroupEntry {
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

                        let (diffuse_data, _fullbright_data) =
                            state.palette.translate(frame.indices());
                        let diffuse_texture =
                            state.create_texture(None, w, h, &TextureData::Diffuse(diffuse_data));
                        let diffuse_view = diffuse_texture.create_default_view();
                        let bind_group =
                            state
                                .device()
                                .create_bind_group(&wgpu::BindGroupDescriptor {
                                    label: None,
                                    layout: &state.alias_pipeline().bind_group_layouts()
                                        [BindGroupLayoutId::PerTexture as usize - 2],
                                    entries: &[wgpu::BindGroupEntry {
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

    pub fn record_draw<'a>(
        &'a self,
        state: &'a GraphicsState,
        pass: &mut wgpu::RenderPass<'a>,
        time: Duration,
        keyframe_id: usize,
        texture_id: usize,
    ) {
        pass.set_pipeline(state.alias_pipeline().pipeline());
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

        pass.set_bind_group(
            BindGroupLayoutId::PerTexture as u32,
            self.textures[texture_id].animate(time),
            &[],
        );
        pass.draw(self.keyframes[keyframe_id].animate(time), 0..1)
    }
}
