use std::{mem::size_of, num::NonZeroU32};

use crate::{
    client::render::{
        ui::{
            layout::{Anchor, ScreenPosition},
            quad::{QuadPipeline, QuadVertex},
            screen_space_vertex_scale, screen_space_vertex_translate,
        },
        Extent2d, GraphicsState, Pipeline, TextureData,
    },
    common::util::any_slice_as_bytes,
};

use cgmath::Vector2;

pub const GLYPH_WIDTH: usize = 8;
pub const GLYPH_HEIGHT: usize = 8;
const GLYPH_COLS: usize = 16;
const GLYPH_ROWS: usize = 16;
const GLYPH_COUNT: usize = GLYPH_ROWS * GLYPH_COLS;
const GLYPH_TEXTURE_WIDTH: usize = GLYPH_WIDTH * GLYPH_COLS;

/// The maximum number of glyphs that can be rendered at once.
pub const MAX_INSTANCES: usize = 65536;

lazy_static! {
    static ref VERTEX_BUFFER_ATTRIBUTES: [Vec<wgpu::VertexAttribute>; 2] = [
        wgpu::vertex_attr_array![
            0 => Float32x2, // a_position
            1 => Float32x2 // a_texcoord
        ].to_vec(),
        wgpu::vertex_attr_array![
            2 => Float32x2, // a_instance_position
            3 => Float32x2, // a_instance_scale
            4 => Uint32 // a_instance_layer
        ].to_vec(),
    ];
}

pub struct GlyphPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layouts: Vec<wgpu::BindGroupLayout>,
    instance_buffer: wgpu::Buffer,
}

impl GlyphPipeline {
    pub fn new(
        device: &wgpu::Device,
        compiler: &mut shaderc::Compiler,
        sample_count: u32,
    ) -> GlyphPipeline {
        let (pipeline, bind_group_layouts) =
            GlyphPipeline::create(device, compiler, &[], sample_count);

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("quad instance buffer"),
            size: (MAX_INSTANCES * size_of::<GlyphInstance>()) as u64,
            usage: wgpu::BufferUsage::VERTEX | wgpu::BufferUsage::COPY_DST,
            mapped_at_creation: false,
        });

        GlyphPipeline {
            pipeline,
            bind_group_layouts,
            instance_buffer,
        }
    }

    pub fn rebuild(
        &mut self,
        device: &wgpu::Device,
        compiler: &mut shaderc::Compiler,
        sample_count: u32,
    ) {
        let layout_refs = self.bind_group_layouts.iter().collect::<Vec<_>>();
        self.pipeline = GlyphPipeline::recreate(device, compiler, &layout_refs, sample_count);
    }

    pub fn pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }

    pub fn bind_group_layouts(&self) -> &[wgpu::BindGroupLayout] {
        &self.bind_group_layouts
    }

    pub fn instance_buffer(&self) -> &wgpu::Buffer {
        &self.instance_buffer
    }
}

const BIND_GROUP_LAYOUT_ENTRIES: &[wgpu::BindGroupLayoutEntry] = &[
    // sampler
    wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStage::FRAGMENT,
        ty: wgpu::BindingType::Sampler {
            filtering: true,
            comparison: false,
        },
        count: None,
    },
    // glyph texture array
    wgpu::BindGroupLayoutEntry {
        binding: 1,
        visibility: wgpu::ShaderStage::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            view_dimension: wgpu::TextureViewDimension::D2,
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            multisampled: false,
        },
        count: NonZeroU32::new(GLYPH_COUNT as u32),
    },
];

impl Pipeline for GlyphPipeline {
    type VertexPushConstants = ();
    type SharedPushConstants = ();
    type FragmentPushConstants = ();

    fn name() -> &'static str {
        "glyph"
    }

    fn vertex_shader() -> &'static str {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/shaders/glyph.vert"))
    }

    fn fragment_shader() -> &'static str {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/shaders/glyph.frag"))
    }

    fn primitive_state() -> wgpu::PrimitiveState {
        QuadPipeline::primitive_state()
    }

    fn bind_group_layout_descriptors() -> Vec<wgpu::BindGroupLayoutDescriptor<'static>> {
        vec![wgpu::BindGroupLayoutDescriptor {
            label: Some("glyph constant bind group"),
            entries: BIND_GROUP_LAYOUT_ENTRIES,
        }]
    }

    fn color_target_states() -> Vec<wgpu::ColorTargetState> {
        QuadPipeline::color_target_states()
    }

    fn depth_stencil_state() -> Option<wgpu::DepthStencilState> {
        QuadPipeline::depth_stencil_state()
    }

    fn vertex_buffer_layouts() -> Vec<wgpu::VertexBufferLayout<'static>> {
        vec![
            wgpu::VertexBufferLayout {
                array_stride: size_of::<QuadVertex>() as u64,
                step_mode: wgpu::InputStepMode::Vertex,
                attributes: &VERTEX_BUFFER_ATTRIBUTES[0],
            },
            wgpu::VertexBufferLayout {
                array_stride: size_of::<GlyphInstance>() as u64,
                step_mode: wgpu::InputStepMode::Instance,
                attributes: &VERTEX_BUFFER_ATTRIBUTES[1],
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
                let (diffuse_data, _) = state.palette().translate(indices);
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
            .map(|tex| tex.create_view(&Default::default()))
            .collect::<Vec<_>>();
        let texture_view_refs = texture_views.iter().collect::<Vec<_>>();

        let const_bind_group = state
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("glyph constant bind group"),
                layout: &state.glyph_pipeline().bind_group_layouts()[0],
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Sampler(state.diffuse_sampler()),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureViewArray(&texture_view_refs[..]),
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
        target_size: Extent2d,
    ) -> Vec<GlyphInstance> {
        let mut instances = Vec::new();
        let Extent2d {
            width: display_width,
            height: display_height,
        } = target_size;
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

    pub fn record_draw<'a>(
        &'a self,
        state: &'a GraphicsState,
        pass: &mut wgpu::RenderPass<'a>,
        target_size: Extent2d,
        commands: &[GlyphRendererCommand],
    ) {
        let instances = self.generate_instances(commands, target_size);
        state
            .queue()
            .write_buffer(state.glyph_pipeline().instance_buffer(), 0, unsafe {
                any_slice_as_bytes(&instances)
            });
        pass.set_pipeline(state.glyph_pipeline().pipeline());
        pass.set_vertex_buffer(0, state.quad_pipeline().vertex_buffer().slice(..));
        pass.set_vertex_buffer(1, state.glyph_pipeline().instance_buffer().slice(..));
        pass.set_bind_group(0, &self.const_bind_group, &[]);
        pass.draw(0..6, 0..commands.len() as u32);
    }
}
