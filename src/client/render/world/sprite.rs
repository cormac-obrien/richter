use std::mem::size_of;

use crate::{
    client::render::{
        world::{BindGroupLayoutId, WorldPipelineBase},
        GraphicsState, Pipeline, TextureData,
    },
    common::{
        sprite::{SpriteFrame, SpriteKind, SpriteModel, SpriteSubframe},
        util::any_slice_as_bytes,
    },
};

use chrono::Duration;

pub struct SpritePipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layouts: Vec<wgpu::BindGroupLayout>,
    vertex_buffer: wgpu::Buffer,
}

impl SpritePipeline {
    pub fn new(
        device: &wgpu::Device,
        compiler: &mut shaderc::Compiler,
        world_bind_group_layouts: &[wgpu::BindGroupLayout],
        sample_count: u32,
    ) -> SpritePipeline {
        let (pipeline, bind_group_layouts) =
            SpritePipeline::create(device, compiler, world_bind_group_layouts, sample_count);

        use wgpu::util::DeviceExt as _;
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: unsafe { any_slice_as_bytes(&VERTICES) },
            usage: wgpu::BufferUsage::VERTEX,
        });

        SpritePipeline {
            pipeline,
            bind_group_layouts,
            vertex_buffer,
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
        self.pipeline = SpritePipeline::recreate(device, compiler, &layout_refs, sample_count);
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
}

lazy_static! {
    static ref VERTEX_BUFFER_ATTRIBUTES: [wgpu::VertexAttributeDescriptor; 3] =
        wgpu::vertex_attr_array![
            // position
            0 => Float3,
            // normal
            1 => Float3,
            // texcoord
            2 => Float2,
        ];
}

impl Pipeline for SpritePipeline {
    type VertexPushConstants = ();
    type SharedPushConstants = ();
    type FragmentPushConstants = ();

    fn name() -> &'static str {
        "sprite"
    }

    fn vertex_shader() -> &'static str {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/shaders/sprite.vert"))
    }

    fn fragment_shader() -> &'static str {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/shaders/sprite.frag"))
    }

    // NOTE: if any of the binding indices are changed, they must also be changed in
    // the corresponding shaders and the BindGroupLayout generation functions.
    fn bind_group_layout_descriptors() -> Vec<wgpu::BindGroupLayoutDescriptor<'static>> {
        vec![
            // group 2: updated per-texture
            wgpu::BindGroupLayoutDescriptor {
                label: Some("sprite per-texture chain bind group"),
                entries: &[
                    // diffuse texture, updated once per face
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStage::FRAGMENT,
                        ty: wgpu::BindingType::SampledTexture {
                            dimension: wgpu::TextureViewDimension::D2,
                            component_type: wgpu::TextureComponentType::Float,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
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
            stride: size_of::<SpriteVertex>() as u64,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: &VERTEX_BUFFER_ATTRIBUTES[..],
        }]
    }
}

// these type aliases are here to aid readability of e.g. size_of::<Position>()
type Position = [f32; 3];
type Normal = [f32; 3];
type DiffuseTexcoord = [f32; 2];

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SpriteVertex {
    position: Position,
    normal: Normal,
    diffuse_texcoord: DiffuseTexcoord,
}

pub const VERTICES: [SpriteVertex; 6] = [
    SpriteVertex {
        position: [0.0, 0.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        diffuse_texcoord: [0.0, 1.0],
    },
    SpriteVertex {
        position: [0.0, 1.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        diffuse_texcoord: [0.0, 0.0],
    },
    SpriteVertex {
        position: [1.0, 1.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        diffuse_texcoord: [1.0, 0.0],
    },
    SpriteVertex {
        position: [0.0, 0.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        diffuse_texcoord: [0.0, 1.0],
    },
    SpriteVertex {
        position: [1.0, 1.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        diffuse_texcoord: [1.0, 0.0],
    },
    SpriteVertex {
        position: [1.0, 0.0, 0.0],
        normal: [0.0, 0.0, 1.0],
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
            let (diffuse_data, _fullbright_data) = state.palette.translate(subframe.indexed());
            let diffuse = state.create_texture(
                None,
                subframe.width(),
                subframe.height(),
                &TextureData::Diffuse(diffuse_data),
            );
            let diffuse_view = diffuse.create_view(&Default::default());
            let bind_group = state
                .device()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &state.sprite_pipeline().bind_group_layouts()
                        [BindGroupLayoutId::PerTexture as usize - 2],
                    entries: &[wgpu::BindGroupEntry {
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

    pub fn record_draw<'a>(
        &'a self,
        state: &'a GraphicsState,
        pass: &mut wgpu::RenderPass<'a>,
        frame_id: usize,
        time: Duration,
    ) {
        pass.set_pipeline(state.sprite_pipeline().pipeline());
        pass.set_vertex_buffer(0, state.sprite_pipeline().vertex_buffer().slice(..));
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
