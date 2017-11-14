use std::collections::HashMap;
use std::iter::IntoIterator;
use std::sync::Arc;

use bsp;
use engine;

use vulkano::buffer::BufferAccess;
use vulkano::buffer::BufferSlice;
use vulkano::buffer::BufferUsage;
use vulkano::buffer::ImmutableBuffer;
use vulkano::device::Device;
use vulkano::device::Queue;
use vulkano::format::R8G8B8A8Unorm;
use vulkano::image::Dimensions;
use vulkano::image::ImmutableImage;
use vulkano::image::ImageViewAccess;
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
    } pcs;

    vec3 quake_to_vulkan(vec3 v) {
        return vec3(-v.y, -v.z, -v.x);
    }

    void main() {
        f_texcoord = vec2(
            (dot(v_position, pcs.s_vector) + pcs.s_offset) / pcs.tex_w,
            (dot(v_position, pcs.t_vector) + pcs.t_offset) / pcs.tex_h
        );
        gl_Position = pcs.u_projection * vec4(quake_to_vulkan(v_position), 1.0);
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
    } pcs;

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
}

impl VkBsp {
    pub fn new(
        bsp: bsp::Bsp,
        device: Arc<Device>,
        queue: Arc<Queue>,
    ) -> Result<VkBsp, bsp::BspError> {
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
        })
    }
}
