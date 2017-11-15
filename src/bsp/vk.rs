use std::collections::HashMap;
use std::iter::IntoIterator;
use std::sync::Arc;

use bsp;
use engine;

use vulkano::buffer::BufferAccess;
use vulkano::buffer::BufferSlice;
use vulkano::buffer::BufferUsage;
use vulkano::buffer::ImmutableBuffer;
use vulkano::command_buffer::AutoCommandBuffer;
use vulkano::command_buffer::AutoCommandBufferBuilder;
use vulkano::command_buffer::DynamicState;
use vulkano::descriptor::PipelineLayoutAbstract;
use vulkano::descriptor::descriptor_set::FixedSizeDescriptorSetsPool;
use vulkano::device::Device;
use vulkano::device::Queue;
use vulkano::format::R8G8B8A8Unorm;
use vulkano::framebuffer::RenderPassAbstract;
use vulkano::framebuffer::Subpass;
use vulkano::image::Dimensions;
use vulkano::image::ImmutableImage;
use vulkano::image::ImageViewAccess;
use vulkano::pipeline::GraphicsPipeline;
use vulkano::pipeline::GraphicsPipelineAbstract;
use vulkano::pipeline::viewport::Viewport;
use vulkano::sampler::Sampler;
use vulkano::sync::GpuFuture;

//   | +z                 | -y
//   |                    |
//   |_____ -y Quake ->   |_____ +x Vulkan
//   /                    /
//  /                    /
// / +x                 / -z
//
// Quake  [x, y, z] <-> [-z, -x, -y] Vulkan
// Vulkan [x, y, z] <-> [ y, -z, -x] Quake

mod vs {
    #[derive(VulkanoShader)]
    #[ty = "vertex"]
    #[src = "
    #version 450

    layout (location = 0) in vec3 v_position;

    layout (location = 0) out vec2 f_texcoord;

    layout (push_constant) uniform PushConstants {
        mat4 u_projection;
        vec3 s_vector;
        float s_offset;
        vec3 t_vector;
        float t_offset;
        float tex_w;
        float tex_h;
    } push_constants;

    vec3 quake_to_vulkan(vec3 v) {
        return vec3(-v.y, -v.z, -v.x);
    }

    void main() {
        f_texcoord = vec2(
            (dot(v_position, push_constants.s_vector) + push_constants.s_offset)
                / push_constants.tex_w,
            (dot(v_position, push_constants.t_vector) + push_constants.t_offset)
                / push_constants.tex_h
        );
        gl_Position = push_constants.u_projection * vec4(quake_to_vulkan(v_position), 1.0);
    }
    "]
    struct Dummy;
}

mod fs {
    #[derive(VulkanoShader)]
    #[ty = "fragment"]
    #[src = "
    #version 450

    layout (location = 0) in vec2 f_texcoord;

    layout (location = 0) out vec4 out_color;

    layout (set = 0, binding = 0) uniform sampler2D u_sampler;

    layout (push_constant) uniform PushConstants {
        mat4 u_projection;
        vec3 s_vector;
        float s_offset;
        vec3 t_vector;
        float t_offset;
        float tex_w;
        float tex_h;
    } push_constants;

    void main() {
        out_color = texture(u_sampler, f_texcoord);
    }"]
    struct Dummy;
}

type VkBspPlane = bsp::BspPlane;

struct VkBspTexture {
    img: Arc<ImmutableImage<R8G8B8A8Unorm>>,
    next: Option<usize>,
}

struct VkBspVertex {
    v_position: [f32; 3],
}
impl_vertex!(VkBspVertex, v_position);

type VkBspNodeChild = bsp::BspNodeChild;
type VkBspNode = bsp::BspNode;
type VkBspTexInfo = bsp::BspTexInfo;
type VkBspFaceSide = bsp::BspFaceSide;

struct VkBspFace {
    plane_id: usize,
    side: VkBspFaceSide,
    index_slice: Arc<BufferSlice<[u32], Arc<ImmutableBuffer<[u32]>>>>,
    texinfo_id: usize,
    light_styles: [u8; bsp::MAX_LIGHTSTYLES],
    lightmap_id: Option<usize>,
}

type VkBspClipNodeCollision = bsp::BspClipNodeCollision;
type VkBspClipNode = bsp::BspClipNode;
type VkBspLeaf = bsp::BspLeaf;
type VkBspModel = bsp::BspModel;

pub struct VkBsp {
    entities: Vec<HashMap<String, String>>,
    planes: Vec<VkBspPlane>,
    textures: Vec<VkBspTexture>,
    vertex_buffer: Arc<ImmutableBuffer<[VkBspVertex]>>,
    index_buffer: Arc<ImmutableBuffer<[u32]>>,
    visibility: Vec<u8>,
    nodes: Vec<VkBspNode>,
    texinfo: Vec<VkBspTexInfo>,
    faces: Vec<VkBspFace>,
    lightmaps: Vec<u8>,
    clipnodes: Vec<VkBspClipNode>,
    leaves: Vec<VkBspLeaf>,
    facelist: Vec<usize>,
    queue: Arc<Queue>,
    pipeline: Arc<GraphicsPipelineAbstract + Send + Sync>,
    vertex_shader: vs::Shader,
    fragment_shader: fs::Shader,
    sampler: Arc<Sampler>,
}

impl VkBsp {
    pub fn new<R>(
        bsp: bsp::Bsp,
        queue: Arc<Queue>,
        subpass: Subpass<R>,
        sampler: Arc<Sampler>,
    ) -> Result<VkBsp, bsp::BspError>
    where
        R: RenderPassAbstract + Send + Sync + 'static,
    {
        let device = queue.device();
        let mut textures: Vec<VkBspTexture> = Vec::new();

        for tex in bsp.textures.iter() {
            let rgba = engine::indexed_to_rgba(&tex.mipmaps[0]);

            // TODO: currently does not support mipmaps
            let (img, img_future) = ImmutableImage::from_iter(
                rgba.into_iter(),
                Dimensions::Dim2d {
                    width: tex.width as u32,
                    height: tex.height as u32,
                },
                R8G8B8A8Unorm,
                queue.clone(),
            ).unwrap();

            // TODO: chain these futures
            img_future.flush().expect(
                "Failed to flush texture creation operations",
            );

            textures.push(VkBspTexture {
                img: img,
                next: tex.next,
            });
        }

        let (vertex_buffer, vb_future) =
            ImmutableBuffer::from_iter(
                bsp.vertices.iter().map(|&v| VkBspVertex { v_position: v }),
                BufferUsage::vertex_buffer(),
                queue.clone(),
            ).unwrap();

        vb_future.flush().expect(
            "Failed to flush vertex buffer creation",
        );

        let (index_buffer, ib_future) = ImmutableBuffer::from_iter(
            bsp.edgelist.iter().map(|ref e| {
                bsp.edges[e.index].vertex_ids[match e.direction {
                                                  bsp::BspEdgeDirection::Forward => 0,
                                                  bsp::BspEdgeDirection::Backward => 1,
                                              }] as u32
            }),
            BufferUsage::index_buffer(),
            queue.clone(),
        ).unwrap();

        ib_future.flush().expect(
            "Failed to flush index buffer creation",
        );

        let mut faces = Vec::new();
        for face in bsp.faces.iter() {
            faces.push(VkBspFace {
                plane_id: face.plane_id,
                side: face.side,
                index_slice: Arc::new(
                    BufferSlice::from(index_buffer.clone().into_buffer_slice())
                        .slice(face.edge_id..face.edge_id + face.edge_count)
                        .unwrap(),
                ),
                texinfo_id: face.texinfo_id,
                light_styles: face.light_styles,
                lightmap_id: face.lightmap_id,
            });
        }

        let vertex_shader = vs::Shader::load(device.clone()).unwrap();
        let fragment_shader = fs::Shader::load(device.clone()).unwrap();

        let pipeline = Arc::new(
            GraphicsPipeline::start()
                .vertex_input_single_buffer::<VkBspVertex>()
                .vertex_shader(vertex_shader.main_entry_point(), ())
                .triangle_fan()
                .viewports_dynamic_scissors_irrelevant(1)
                .fragment_shader(fragment_shader.main_entry_point(), ())
                .depth_stencil_simple_depth()
                .front_face_clockwise()
                .cull_mode_back()
                .render_pass(subpass)
                .build(device.clone())
                .expect("Failed to create graphics pipeline"),
        );

        Ok(VkBsp {
            entities: bsp.entities,
            planes: bsp.planes,
            textures: textures,
            vertex_buffer: vertex_buffer.clone(),
            index_buffer: index_buffer.clone(),
            visibility: bsp.visibility,
            nodes: bsp.nodes,
            texinfo: bsp.texinfo,
            faces: faces,
            lightmaps: bsp.lightmaps,
            clipnodes: bsp.clipnodes,
            leaves: bsp.leaves,
            facelist: bsp.facelist,
            queue: queue.clone(),
            pipeline: pipeline,
            vertex_shader: vs::Shader::load(device.clone()).unwrap(),
            fragment_shader: fs::Shader::load(device.clone()).unwrap(),
            sampler: sampler.clone(),
        })
    }

    pub fn draw_naive<R>(
        &mut self,
        viewport_dimensions: [u32; 2],
        projection_matrix: [[f32; 4]; 4],
        render_pass: Subpass<R>,
    ) -> AutoCommandBuffer
    where
        R: RenderPassAbstract + Clone + Send + Sync + 'static,
    {
        let mut push_constants = vs::ty::PushConstants {
            u_projection: [[0.0; 4]; 4],
            s_vector: [0.0; 3],
            s_offset: 0.0,
            t_vector: [0.0; 3],
            t_offset: 0.0,
            tex_w: 0.0,
            tex_h: 0.0,
        };

        let dynamic_state = DynamicState {
            line_width: None,
            viewports: Some(vec![
                Viewport {
                    origin: [0.0, 0.0],
                    dimensions: [viewport_dimensions[0] as f32, viewport_dimensions[1] as f32],
                    depth_range: 0.0..1.0,
                },
            ]),
            scissors: None,
        };

        let mut cmd_buf_builder = AutoCommandBufferBuilder::secondary_graphics_one_time_submit(
            self.queue.device().clone(),
            self.queue.family(),
            render_pass,
        ).unwrap();

        let mut descriptor_pool = FixedSizeDescriptorSetsPool::new(self.pipeline.clone(), 0);

        for face in self.faces.iter() {
            let texinfo = &self.texinfo[face.texinfo_id];
            let tex_dimensions = &self.textures[texinfo.tex_id].img.dimensions();
            push_constants.u_projection = projection_matrix;
            push_constants.s_vector = texinfo.s_vector;
            push_constants.s_offset = texinfo.s_offset;
            push_constants.t_vector = texinfo.t_vector;
            push_constants.t_offset = texinfo.t_offset;
            push_constants.tex_w = tex_dimensions.width() as f32;
            push_constants.tex_h = tex_dimensions.height() as f32;

            let descriptor_set = descriptor_pool
                .next()
                .add_sampled_image(
                    self.textures[texinfo.tex_id].img.clone(),
                    self.sampler.clone(),
                )
                .unwrap()
                .build()
                .unwrap();

            cmd_buf_builder = cmd_buf_builder
                .draw_indexed(
                    self.pipeline.clone(),
                    dynamic_state.clone(),
                    vec![self.vertex_buffer.clone()],
                    face.index_slice.clone(),
                    descriptor_set,
                    push_constants,
                )
                .unwrap()
                .end_render_pass()
                .unwrap();
        }

        cmd_buf_builder.build().unwrap()
    }
}
