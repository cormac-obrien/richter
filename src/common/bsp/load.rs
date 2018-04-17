// Copyright © 2018 Cormac O'Brien
//
// Permission is hereby granted, free of charge, to any person obtaining a copy of this software
// and associated documentation files (the "Software"), to deal in the Software without
// restriction, including without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all copies or
// substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING
// BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
// NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
// DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

use std::io::BufRead;
use std::io::BufReader;
use std::io::Cursor;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::rc::Rc;

use common::bsp::BspCollisionHull;
use common::bsp::BspCollisionNode;
use common::bsp::BspCollisionNodeChild;
use common::bsp::BspData;
use common::bsp::BspEdge;
use common::bsp::BspEdgeDirection;
use common::bsp::BspEdgeIndex;
use common::bsp::BspFace;
use common::bsp::BspFaceSide;
use common::bsp::BspLeaf;
use common::bsp::BspLeafContents;
use common::bsp::BspModel;
use common::bsp::BspRenderNode;
use common::bsp::BspRenderNodeChild;
use common::bsp::BspTexInfo;
use common::bsp::BspTexture;
use common::bsp::BspTextureAnimation;
use common::bsp::MAX_HULLS;
use common::bsp::MAX_LIGHTSTYLES;
use common::bsp::MIPLEVELS;
use common::math::Axis;
use common::math::Hyperplane;
use common::model::Model;

use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use chrono::Duration;
use cgmath::InnerSpace;
use cgmath::Vector3;
use failure::Error;
use failure::ResultExt;
use num::FromPrimitive;

const VERSION: i32 = 29;

pub const MAX_MODELS: usize = 256;
const MAX_LEAVES: usize = 32767;

// these are only used by QuakeEd
const _MAX_BRUSHES: usize = 4096;
const _MAX_ENTITIES: usize = 1024;

const MAX_ENTSTRING: usize = 65536;
const MAX_PLANES: usize = 8192;
const MAX_RENDER_NODES: usize = 32767;
const MAX_COLLISION_NODES: usize = 32767;
const MAX_VERTICES: usize = 65535;
const MAX_FACES: usize = 65535;
const MAX_MARKTEXINFO: usize = 65535;
const MAX_TEXINFO: usize = 4096;
const MAX_EDGES: usize = 256000;
const MAX_EDGELIST: usize = 512000;
const MAX_TEXTURES: usize = 0x200000;
const MAX_LIGHTMAP: usize = 0x100000;
const MAX_VISLIST: usize = 0x100000;

const PLANE_SIZE: usize = 20;
const RENDER_NODE_SIZE: usize = 24;
const LEAF_SIZE: usize = 28;
const TEXINFO_SIZE: usize = 40;
const FACE_SIZE: usize = 20;
const COLLISION_NODE_SIZE: usize = 8;
const FACELIST_SIZE: usize = 2;
const EDGE_SIZE: usize = 4;
const EDGELIST_SIZE: usize = 4;
const MODEL_SIZE: usize = 64;
const VERTEX_SIZE: usize = 12;
const TEX_NAME_MAX: usize = 16;

const NUM_AMBIENTS: usize = 4;
const MAX_TEXTURE_FRAMES: usize = 10;
const TEXTURE_FRAME_LEN_MS: i64 = 200;

const ASCII_0: usize = '0' as usize;
const ASCII_9: usize = '9' as usize;
const ASCII_CAPITAL_A: usize = 'A' as usize;
const ASCII_CAPITAL_J: usize = 'J' as usize;
const ASCII_SMALL_A: usize = 'a' as usize;
const ASCII_SMALL_J: usize = 'j' as usize;

#[derive(Debug, FromPrimitive)]
enum BspLumpId {
    Entities = 0,
    Planes = 1,
    Textures = 2,
    Vertices = 3,
    Visibility = 4,
    RenderNodes = 5,
    TextureInfo = 6,
    Faces = 7,
    Lightmaps = 8,
    CollisionNodes = 9,
    Leaves = 10,
    FaceList = 11,
    Edges = 12,
    EdgeList = 13,
    Models = 14,
    Count = 15,
}

struct BspLump {
    offset: u64,
    size: usize,
}

impl BspLump {
    fn from_i32s(offset: i32, size: i32) -> Result<BspLump, Error> {
        ensure!(offset >= 0, "Lump offset must not be negative (was {})", offset);
        ensure!(size >= 0, "Lump size must not be negative (was {})", size);

        Ok(BspLump {
            offset: offset as u64,
            size: size as usize,
        })
    }
}

fn check_alignment<S>(seeker: &mut S, ofs: u64) -> Result<(), Error>
where
    S: Seek,
{
    ensure!(
        seeker.seek(SeekFrom::Current(0))? == seeker.seek(SeekFrom::Start(ofs))?,
        "BSP read misaligned"
    );

    Ok(())
}

fn load_hyperplane<R>(reader: &mut R) -> Result<Hyperplane, Error>
where
    R: ReadBytesExt,
{
    let normal = Vector3::new(
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?,
    );

    let dist = reader.read_f32::<LittleEndian>()?;

    let plane = match Axis::from_i32(reader.read_i32::<LittleEndian>()?) {
        Some(ax) => match ax {
            Axis::X => Hyperplane::axis_x(dist),
            Axis::Y => Hyperplane::axis_y(dist),
            Axis::Z => Hyperplane::axis_z(dist),
        }

        None => Hyperplane::new(normal, dist),
    };

    Ok(plane)
}

fn load_texture<R>(mut reader: &mut R, tex_lump_ofs: u64, tex_ofs: u64) -> Result<BspTexture, Error>
where
    R: ReadBytesExt + Seek,
{
    let mut tex_name_bytes = [0u8; TEX_NAME_MAX];
    reader.read(&mut tex_name_bytes)?;
    let len = tex_name_bytes
        .iter()
        .enumerate()
        .find(|&item| item.1 == &0)
        .unwrap_or((TEX_NAME_MAX, &0))
        .0;
    let tex_name = String::from_utf8(tex_name_bytes[..len].to_vec())?;

    let width = reader.read_u32::<LittleEndian>()?;
    let height = reader.read_u32::<LittleEndian>()?;

    let mut mip_offsets = [0usize; MIPLEVELS];
    for m in 0..MIPLEVELS {
        mip_offsets[m] = reader.read_u32::<LittleEndian>()? as usize;
    }

    let mut mipmaps = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    for m in 0..MIPLEVELS {
        let factor = 2usize.pow(m as u32);
        let mipmap_size = (width as usize / factor) * (height as usize / factor);
        let offset = tex_lump_ofs + tex_ofs + mip_offsets[m] as u64;
        reader.seek(SeekFrom::Start(offset))?;
        (&mut reader)
            .take(mipmap_size as u64)
            .read_to_end(&mut mipmaps[m])?;
    }

    Ok(BspTexture {
        name: tex_name,
        width: width,
        height: height,
        mipmaps: mipmaps,
        animation: None,
    })
}

fn load_render_node<R>(reader: &mut R) -> Result<BspRenderNode, Error>
where
    R: ReadBytesExt
{
    let plane_id = reader.read_i32::<LittleEndian>()?;
    if plane_id < 0 {
        bail!("Invalid plane id");
    }

    // If the child ID is positive, it points to another internal node. If it is negative, its
    // bitwise negation points to a leaf node.

    let front = match reader.read_i16::<LittleEndian>()? {
        f if f < 0 => BspRenderNodeChild::Leaf((!f) as usize),
        f => BspRenderNodeChild::Node(f as usize),
    };

    let back = match reader.read_i16::<LittleEndian>()? {
        b if b < 0 => BspRenderNodeChild::Leaf((!b) as usize),
        b => BspRenderNodeChild::Node(b as usize),
    };

    let min = [
        reader.read_i16::<LittleEndian>()?,
        reader.read_i16::<LittleEndian>()?,
        reader.read_i16::<LittleEndian>()?,
    ];

    let max = [
        reader.read_i16::<LittleEndian>()?,
        reader.read_i16::<LittleEndian>()?,
        reader.read_i16::<LittleEndian>()?,
    ];

    let face_id = reader.read_i16::<LittleEndian>()?;
    if face_id < 0 {
        bail!("Invalid face id");
    }

    let face_count = reader.read_u16::<LittleEndian>()?;
    if face_count as usize > MAX_FACES {
        bail!("Invalid face count");
    }

    Ok(BspRenderNode {
        plane_id: plane_id as usize,
        children: [front, back],
        min: min,
        max: max,
        face_id: face_id as usize,
        face_count: face_count as usize,
    })
}

fn load_texinfo<R>(reader: &mut R, texture_count: usize) -> Result<BspTexInfo, Error>
where
    R: ReadBytesExt
{
    let s_vector = Vector3::new(
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?,
    );

    let s_offset = reader.read_f32::<LittleEndian>()?;

    let t_vector = Vector3::new(
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?,
    );

    let t_offset = reader.read_f32::<LittleEndian>()?;

    let tex_id = match reader.read_i32::<LittleEndian>()? {
        t if t < 0 || t as usize > texture_count => bail!("Invalid texture ID"),
        t => t as usize,
    };

    let special = match reader.read_i32::<LittleEndian>()? {
        0 => false,
        1 => true,
        _ => bail!("Invalid texture flags"),
    };

    Ok(BspTexInfo {
        s_vector,
        s_offset,
        t_vector,
        t_offset,

        tex_id,
        special,
    })
}

/// Load a BSP file, returning the models it contains and a `String` describing the entities
/// it contains.
pub fn load(data: &[u8]) -> Result<(Vec<Model>, String), Error> {
    let mut reader = BufReader::new(Cursor::new(data));

    let version = reader.read_i32::<LittleEndian>()?;
    ensure!(version == VERSION, "Bad version number (found {}, should be {})", version, VERSION);

    let mut lumps = Vec::with_capacity(BspLumpId::Count as usize);
    for l in 0..(BspLumpId::Count as usize) {
        let offset = match reader.read_i32::<LittleEndian>()? {
            o if o < 0 => bail!("Invalid lump offset of {}", o),
            o => o,
        };

        let size = match reader.read_i32::<LittleEndian>()? {
            o if o < 0 => bail!("Invalid lump size of {}", o),
            o => o,
        };

        debug!(
            "{: <14} Offset = 0x{:>08x} | Size = 0x{:>08x}",
            format!("{:?}:", BspLumpId::from_usize(l).unwrap()),
            offset,
            size
        );

        lumps.push(BspLump::from_i32s(offset, size).context("Failed to read lump")?);
    }

    let ent_lump = &lumps[BspLumpId::Entities as usize];
    let plane_lump = &lumps[BspLumpId::Planes as usize];
    let tex_lump = &lumps[BspLumpId::Textures as usize];
    let vert_lump = &lumps[BspLumpId::Vertices as usize];
    let vis_lump = &lumps[BspLumpId::Visibility as usize];
    let texinfo_lump = &lumps[BspLumpId::TextureInfo as usize];
    let face_lump = &lumps[BspLumpId::Faces as usize];
    let lightmap_lump = &lumps[BspLumpId::Lightmaps as usize];
    let collision_node_lump = &lumps[BspLumpId::CollisionNodes as usize];
    let leaf_lump = &lumps[BspLumpId::Leaves as usize];
    let facelist_lump = &lumps[BspLumpId::FaceList as usize];
    let edge_lump = &lumps[BspLumpId::Edges as usize];
    let edgelist_lump = &lumps[BspLumpId::EdgeList as usize];
    let model_lump = &lumps[BspLumpId::Models as usize];
    let render_node_lump = &lumps[BspLumpId::RenderNodes as usize];

    // check that lump sizes make sense for their types
    ensure!(plane_lump.size % PLANE_SIZE == 0, "Bad plane lump size");
    ensure!(vert_lump.size % VERTEX_SIZE == 0, "Bad vertex lump size");
    ensure!(render_node_lump.size % RENDER_NODE_SIZE == 0, "Bad render node lump size");
    ensure!(texinfo_lump.size % TEXINFO_SIZE == 0, "Bad texinfo lump size");
    ensure!(face_lump.size % FACE_SIZE == 0, "Bad face lump size");
    ensure!(collision_node_lump.size % COLLISION_NODE_SIZE == 0, "Bad collision node lump size");
    ensure!(leaf_lump.size % LEAF_SIZE == 0, "Bad leaf lump size");
    ensure!(facelist_lump.size % FACELIST_SIZE == 0, "Bad facelist lump size");
    ensure!(edge_lump.size % EDGE_SIZE == 0, "Bad edge lump size");
    ensure!(edgelist_lump.size % EDGELIST_SIZE == 0, "Bad edgelist lump size");
    ensure!(model_lump.size % MODEL_SIZE == 0, "Bad model lump size");

    let plane_count = plane_lump.size / PLANE_SIZE;
    let vert_count = vert_lump.size / VERTEX_SIZE;
    let render_node_count = render_node_lump.size / RENDER_NODE_SIZE;
    let texinfo_count = texinfo_lump.size / TEXINFO_SIZE;
    let face_count = face_lump.size / FACE_SIZE;
    let collision_node_count = collision_node_lump.size / COLLISION_NODE_SIZE;
    let leaf_count = leaf_lump.size / LEAF_SIZE;
    let facelist_count = facelist_lump.size / FACELIST_SIZE;
    let edge_count = edge_lump.size / EDGE_SIZE;
    let edgelist_count = edgelist_lump.size / EDGELIST_SIZE;
    let model_count = model_lump.size / MODEL_SIZE;

    // check limits
    ensure!(plane_count <= MAX_PLANES, "Plane count exceeds MAX_PLANES");
    ensure!(vert_count <= MAX_VERTICES, "Vertex count exceeds MAX_VERTICES");
    ensure!(vis_lump.size <= MAX_VISLIST, "Visibility data size exceeds MAX_VISLIST");
    ensure!(render_node_count <= MAX_RENDER_NODES, "Render node count exceeds MAX_RENDER_NODES");
    ensure!(
        collision_node_count <= MAX_COLLISION_NODES,
        "Collision node count exceeds MAX_COLLISION_NODES"
    );
    ensure!(leaf_count <= MAX_LEAVES, "Leaf count exceeds MAX_LEAVES");
    ensure!(edge_count <= MAX_EDGES, "Edge count exceeds MAX_EDGES");
    ensure!(edgelist_count <= MAX_EDGELIST, "Edge list count exceeds MAX_EDGELIST");
    ensure!(model_count > 0, "No brush models (need at least 1 for worldmodel)");
    ensure!(model_count <= MAX_MODELS, "Model count exceeds MAX_MODELS");

    reader.seek(SeekFrom::Start(ent_lump.offset))?;
    let mut ent_data = Vec::with_capacity(MAX_ENTSTRING);
    reader.read_until(0x00, &mut ent_data)?;
    ensure!(ent_data.len() <= MAX_ENTSTRING, "Entity data exceeds MAX_ENTSTRING");
    let ent_string =
        String::from_utf8(ent_data).context("Failed to create string from entity data")?;
    check_alignment(&mut reader, ent_lump.offset + ent_lump.size as u64)?;

    // load planes
    reader.seek(SeekFrom::Start(plane_lump.offset))?;
    let mut planes = Vec::with_capacity(plane_count);
    for _ in 0..plane_count {
        planes.push(load_hyperplane(&mut reader)?);
    }
    let planes_rc = Rc::new(planes.into_boxed_slice());

    check_alignment(&mut reader, plane_lump.offset + plane_lump.size as u64)?;

    // load textures
    reader.seek(SeekFrom::Start(tex_lump.offset))?;
    let tex_count = reader.read_i32::<LittleEndian>()?;
    ensure!(tex_count >= 0 && tex_count as usize <= MAX_TEXTURES, "Invalid texture count");
    let tex_count = tex_count as usize;

    let mut tex_offsets = Vec::with_capacity(tex_count);
    for _ in 0..tex_count {
        let ofs = reader.read_i32::<LittleEndian>()?;

        tex_offsets.push(match ofs {
            o if o < -1 => bail!("negative texture offset ({})", ofs),
            -1 => None,
            o => Some(o as usize),
        });
    }

    let mut textures = Vec::with_capacity(tex_count);
    for t in 0..tex_count {
        let tex_ofs = match tex_offsets[t] {
            Some(o) => o,

            None => {
                textures.push(BspTexture {
                    name: String::new(),
                    width: 0,
                    height: 0,
                    mipmaps: [Vec::new(), Vec::new(), Vec::new(), Vec::new()],
                    animation: None,
                });

                continue;
            }
        };

        reader.seek(SeekFrom::Start(tex_lump.offset + tex_ofs as u64))?;
        let texture = load_texture(&mut reader, tex_lump.offset as u64, tex_ofs as u64)?;
        debug!(
            "Texture {id:>width$}: {name}",
            id = t,
            width = (tex_count as f32).log(10.0) as usize,
            name = texture.name(),
        );

        textures.push(texture);
    }

    check_alignment(&mut reader, tex_lump.offset + tex_lump.size as u64)?;

    debug!("Sequencing textures");
    for t in 0..textures.len() {
        if !textures[t].name.starts_with("+") || textures[t].animation.is_some() {
            continue;
        }

        debug!("Sequencing texture {}", textures[t].name);

        let mut anim1 = [None; MAX_TEXTURE_FRAMES];
        let mut anim2 = [None; MAX_TEXTURE_FRAMES];
        let mut anim1_len;
        let mut anim2_len;

        let mut frame_char = textures[t]
            .name
            .chars()
            .nth(1)
            .expect("Invalid texture name") as usize;

        match frame_char {
            ASCII_0..=ASCII_9 => {
                anim1_len = frame_char - ASCII_0;
                anim2_len = 0;
                anim1[anim1_len] = Some(t);
                anim1_len += 1;
            }

            ASCII_CAPITAL_A..=ASCII_CAPITAL_J | ASCII_SMALL_A..=ASCII_SMALL_J => {
                if frame_char >= ASCII_SMALL_A && frame_char <= ASCII_SMALL_J {
                    frame_char -= ASCII_SMALL_A - ASCII_CAPITAL_A;
                }
                anim2_len = frame_char - ASCII_CAPITAL_A;
                anim1_len = 0;
                anim2[anim2_len] = Some(t);
                anim2_len += 1;
            }

            _ => bail!("Invalid texture frame specifier: U+{:x}", frame_char),
        }

        for t2 in t + 1..textures.len() {
            // check if this texture has the same base name
            if !textures[t2].name.starts_with("+")
                || textures[t2].name[2..] != textures[t].name[2..]
            {
                continue;
            }

            let mut frame_n_char = textures[t2]
                .name
                .chars()
                .nth(1)
                .expect("Invalid texture name") as usize;

            match frame_n_char {
                ASCII_0..=ASCII_9 => {
                    frame_n_char -= ASCII_0;
                    anim1[frame_n_char] = Some(t2);
                    if frame_n_char + 1 > anim1_len {
                        anim1_len = frame_n_char + 1;
                    }
                }

                ASCII_CAPITAL_A..=ASCII_CAPITAL_J | ASCII_SMALL_A..=ASCII_SMALL_J => {
                    if frame_n_char >= ASCII_SMALL_A && frame_n_char <= ASCII_SMALL_J {
                        frame_n_char -= ASCII_SMALL_A - ASCII_CAPITAL_A;
                    }
                    frame_n_char -= ASCII_CAPITAL_A;
                    anim2[frame_n_char] = Some(t2);
                    if frame_n_char + 1 > anim2_len {
                        anim2_len += 1;
                    }
                }

                _ => bail!("Invalid texture frame specifier: U+{:x}", frame_n_char),
            }
        }

        for frame in 0..anim1_len {
            let mut tex2 = match anim1[frame] {
                Some(t2) => t2,
                None => bail!("Missing frame {} of {}", frame, textures[t].name),
            };

            textures[tex2].animation = Some(BspTextureAnimation {
                sequence_duration: Duration::milliseconds(TEXTURE_FRAME_LEN_MS * anim1_len as i64),
                time_start: Duration::milliseconds(TEXTURE_FRAME_LEN_MS * frame as i64),
                time_end: Duration::milliseconds(TEXTURE_FRAME_LEN_MS * (frame as i64 + 1)),
                next: anim1[(frame + 1) % anim1_len].unwrap(),
            });
        }

        for frame in 0..anim2_len {
            let mut tex2 = match anim2[frame] {
                Some(t2) => t2,
                None => bail!("Missing frame {} of {}", frame, textures[t].name),
            };

            textures[tex2].animation = Some(BspTextureAnimation {
                sequence_duration: Duration::milliseconds(TEXTURE_FRAME_LEN_MS * anim2_len as i64),
                time_start: Duration::milliseconds(TEXTURE_FRAME_LEN_MS * frame as i64),
                time_end: Duration::milliseconds(TEXTURE_FRAME_LEN_MS * (frame as i64 + 1)),
                next: anim2[(frame + 1) % anim2_len].unwrap(),
            });
        }
    }

    reader.seek(SeekFrom::Start(vert_lump.offset))?;
    let mut vertices = Vec::with_capacity(vert_count);
    for _ in 0..vert_count {
        vertices.push(Vector3::new(
            reader.read_f32::<LittleEndian>()?,
            reader.read_f32::<LittleEndian>()?,
            reader.read_f32::<LittleEndian>()?,
        ));
    }
    check_alignment(&mut reader, vert_lump.offset + vert_lump.size as u64)?;

    reader.seek(SeekFrom::Start(vis_lump.offset))?;

    // visibility data
    let mut vis_data = Vec::with_capacity(vis_lump.size);
    (&mut reader).take(vis_lump.size as u64).read_to_end(&mut vis_data)?;
    check_alignment(&mut reader, vis_lump.offset + vis_lump.size as u64)?;

    // render nodes
    reader.seek(SeekFrom::Start(render_node_lump.offset))?;
    debug!("Render node count = {}", render_node_count);
    let mut render_nodes = Vec::with_capacity(render_node_count);
    for _ in 0..render_node_count {
        render_nodes.push(load_render_node(&mut reader)?);
    }
    check_alignment(&mut reader, render_node_lump.offset + render_node_lump.size as u64)?;

    // texinfo
    reader.seek(SeekFrom::Start(texinfo_lump.offset))?;
    let mut texinfo = Vec::with_capacity(texinfo_count);
    for _ in 0..texinfo_count {
        texinfo.push(load_texinfo(&mut reader, tex_count)?);
    }
    check_alignment(&mut reader, texinfo_lump.offset + texinfo_lump.size as u64)?;

    reader.seek(SeekFrom::Start(face_lump.offset))?;
    let mut faces = Vec::with_capacity(face_count);
    for _ in 0..face_count {
        let plane_id = reader.read_i16::<LittleEndian>()?;
        if plane_id < 0 || plane_id as usize > plane_count {
            bail!("Invalid plane count");
        }

        let side = match reader.read_i16::<LittleEndian>()? {
            0 => BspFaceSide::Front,
            1 => BspFaceSide::Back,
            _ => bail!("Invalid face side"),
        };

        let edge_id = reader.read_i32::<LittleEndian>()?;
        if edge_id < 0 {
            bail!("Invalid edge ID");
        }

        let edge_count = reader.read_i16::<LittleEndian>()?;
        if edge_count < 3 {
            bail!("Invalid edge count");
        }

        let texinfo_id = reader.read_i16::<LittleEndian>()?;
        if texinfo_id < 0 || texinfo_id as usize > texinfo_count {
            bail!("Invalid texinfo ID");
        }

        let mut light_styles = [0; MAX_LIGHTSTYLES];
        for i in 0..light_styles.len() {
            light_styles[i] = reader.read_u8()?;
        }

        let lightmap_id = match reader.read_i32::<LittleEndian>()? {
            o if o < -1 => bail!("Invalid lightmap offset"),
            -1 => None,
            o => Some(o as usize),
        };

        faces.push(BspFace {
            plane_id: plane_id as usize,
            side: side,
            edge_id: edge_id as usize,
            edge_count: edge_count as usize,
            texinfo_id: texinfo_id as usize,
            light_styles: light_styles,
            lightmap_id: lightmap_id,
            texture_mins: [0, 0],
            extents: [0, 0],
        });
    }
    check_alignment(&mut reader, face_lump.offset + face_lump.size as u64)?;

    reader.seek(SeekFrom::Start(lightmap_lump.offset))?;
    let mut lightmaps = Vec::with_capacity(lightmap_lump.size);
    (&mut reader)
        .take(lightmap_lump.size as u64)
        .read_to_end(&mut lightmaps)?;
    check_alignment(&mut reader, lightmap_lump.offset + lightmap_lump.size as u64)?;

    reader.seek(SeekFrom::Start(collision_node_lump.offset))?;


    let mut collision_nodes = Vec::with_capacity(collision_node_count);
    for _ in 0..collision_node_count {
        let plane_id = match reader.read_i32::<LittleEndian>()? {
            x if x < 0 => bail!("Invalid plane id"),
            x => x as usize,
        };

        let front = match reader.read_i16::<LittleEndian>()? {
            x if x < 0 => match BspLeafContents::from_i16(-x) {
                Some(c) => BspCollisionNodeChild::Contents(c),
                None => bail!("Invalid leaf contents ({})", -x),
            },
            x => BspCollisionNodeChild::Node(x as usize),
        };

        let back = match reader.read_i16::<LittleEndian>()? {
            x if x < 0 => match BspLeafContents::from_i16(-x) {
                Some(c) => BspCollisionNodeChild::Contents(c),
                None => bail!("Invalid leaf contents ({})", -x),
            },
            x => BspCollisionNodeChild::Node(x as usize),
        };

        collision_nodes.push(BspCollisionNode {
            plane_id,
            children: [front, back],
        });
    }

    let collision_nodes_rc = Rc::new(collision_nodes.into_boxed_slice());

    let hull_1 = BspCollisionHull {
        planes: planes_rc.clone(),
        nodes: collision_nodes_rc.clone(),
        node_id: 0,
        node_count: collision_node_count,
        mins: Vector3::new(-16.0, -16.0, -24.0),
        maxs: Vector3::new(16.0, 16.0, 32.0),
    };

    let hull_2 = BspCollisionHull {
        planes: planes_rc.clone(),
        nodes: collision_nodes_rc.clone(),
        node_id: 0,
        node_count: collision_node_count,
        mins: Vector3::new(-32.0, -32.0, -24.0),
        maxs: Vector3::new(32.0, 32.0, 64.0),
    };

    if reader.seek(SeekFrom::Current(0))?
        != reader.seek(SeekFrom::Start(
            collision_node_lump.offset + collision_node_lump.size as u64,
        ))? {
        bail!("BSP read data misaligned");
    }

    reader.seek(SeekFrom::Start(leaf_lump.offset))?;


    let mut leaves = Vec::with_capacity(leaf_count);

    for _ in 0..leaf_count {
        // note the negation here (the constants are negative in the original engine to differentiate
        // them from plane IDs)
        let contents_id = -reader.read_i32::<LittleEndian>()?;

        let contents = match BspLeafContents::from_i32(contents_id) {
            Some(c) => c,
            None => bail!("Invalid leaf contents ({})", contents_id),
        };

        let vis_offset = match reader.read_i32::<LittleEndian>()? {
            x if x < -1 => bail!("Invalid visibility data offset"),
            -1 => None,
            x => Some(x as usize),
        };

        let min = [
            reader.read_i16::<LittleEndian>()?,
            reader.read_i16::<LittleEndian>()?,
            reader.read_i16::<LittleEndian>()?,
        ];

        let max = [
            reader.read_i16::<LittleEndian>()?,
            reader.read_i16::<LittleEndian>()?,
            reader.read_i16::<LittleEndian>()?,
        ];

        let facelist_id = reader.read_u16::<LittleEndian>()? as usize;
        let facelist_count = reader.read_u16::<LittleEndian>()? as usize;
        let mut sounds = [0u8; NUM_AMBIENTS];
        reader.read(&mut sounds)?;
        leaves.push(BspLeaf {
            contents,
            vis_offset,
            min,
            max,
            facelist_id,
            facelist_count,
            sounds,
        });
    }
    check_alignment(&mut reader, leaf_lump.offset + leaf_lump.size as u64)?;

    reader.seek(SeekFrom::Start(facelist_lump.offset))?;
    let mut facelist = Vec::with_capacity(facelist_count);
    for _ in 0..facelist_count {
        facelist.push(reader.read_u16::<LittleEndian>()? as usize);
    }
    if reader.seek(SeekFrom::Current(0))?
        != reader.seek(SeekFrom::Start(
            facelist_lump.offset + facelist_lump.size as u64,
        ))? {
        bail!("BSP read data misaligned");
    }

    reader.seek(SeekFrom::Start(edge_lump.offset))?;
    let mut edges = Vec::with_capacity(edge_count);
    for _ in 0..edge_count {
        edges.push(BspEdge {
            vertex_ids: [
                reader.read_u16::<LittleEndian>()?,
                reader.read_u16::<LittleEndian>()?,
            ],
        });
    }
    check_alignment(&mut reader, edge_lump.offset + edge_lump.size as u64)?;

    reader.seek(SeekFrom::Start(edgelist_lump.offset))?;
    let mut edgelist = Vec::with_capacity(edgelist_count);
    for _ in 0..edgelist_count {
        edgelist.push(match reader.read_i32::<LittleEndian>()? {
            x if x >= 0 => BspEdgeIndex {
                direction: BspEdgeDirection::Forward,
                index: x as usize,
            },

            x if x < 0 => BspEdgeIndex {
                direction: BspEdgeDirection::Backward,
                index: -x as usize,
            },

            x => bail!(format!("Invalid edge index {}", x)),
        });
    }
    if reader.seek(SeekFrom::Current(0))?
        != reader.seek(SeekFrom::Start(
            edgelist_lump.offset + edgelist_lump.size as u64,
        ))? {
        bail!("BSP read data misaligned");
    }

    // see Calc_SurfaceExtents,
    // https://github.com/id-Software/Quake/blob/master/WinQuake/gl_model.c#L705-L749

    for (face_id, face) in faces.iter_mut().enumerate() {
        let texinfo = &texinfo[face.texinfo_id];

        let mut s_min = ::std::f32::INFINITY;
        let mut t_min = ::std::f32::INFINITY;
        let mut s_max = ::std::f32::NEG_INFINITY;
        let mut t_max = ::std::f32::NEG_INFINITY;

        for edge_idx in &edgelist[face.edge_id..face.edge_id + face.edge_count] {
            let vertex_id = edges[edge_idx.index].vertex_ids[edge_idx.direction as usize] as usize;
            let vertex = vertices[vertex_id];
            let s = texinfo.s_vector.dot(vertex) + texinfo.s_offset;
            let t = texinfo.t_vector.dot(vertex) + texinfo.t_offset;

            s_min = s_min.min(s);
            s_max = s_max.max(s);
            t_min = t_min.min(t);
            t_max = t_max.max(t);
        }

        let round_down = |f: f32| (f / 16.0).floor() as i16 * 16;
        let round_up = |f: f32| (f / 16.0).ceil() as i16 * 16;

        face.texture_mins = [round_down(s_min), round_down(t_min)];
        face.extents = [round_up(s_max - s_min), round_up(t_max - t_min)];

        if !texinfo.special && (face.extents[0] > 512 || face.extents[1] > 512) {
            bail!("Bad face extents: face {}, texture {}: {:?}", face_id, textures[texinfo.tex_id].name, face.extents);
        }
    }

    // see Mod_MakeHull0,
    // https://github.com/id-Software/Quake/blob/master/WinQuake/gl_model.c#L1001-L1031
    //
    // This essentially duplicates the render nodes into a tree of collision nodes.
    let mut render_as_collision_nodes = Vec::with_capacity(render_nodes.len());
    for i in 0..render_nodes.len() {
        render_as_collision_nodes.push(BspCollisionNode {
            plane_id: render_nodes[i].plane_id,
            children: [
                match render_nodes[i].children[0] {
                    BspRenderNodeChild::Node(n) => BspCollisionNodeChild::Node(n),
                    BspRenderNodeChild::Leaf(l) => {
                        BspCollisionNodeChild::Contents(leaves[l].contents)
                    }
                },
                match render_nodes[i].children[1] {
                    BspRenderNodeChild::Node(n) => BspCollisionNodeChild::Node(n),
                    BspRenderNodeChild::Leaf(l) => {
                        BspCollisionNodeChild::Contents(leaves[l].contents)
                    }
                },
            ],
        })
    }
    let render_as_collision_nodes_rc = Rc::new(render_as_collision_nodes.into_boxed_slice());

    let hull_0 = BspCollisionHull {
        planes: planes_rc.clone(),
        nodes: render_as_collision_nodes_rc.clone(),
        node_id: 0,
        node_count: render_as_collision_nodes_rc.len(),
        mins: Vector3::new(0.0, 0.0, 0.0),
        maxs: Vector3::new(0.0, 0.0, 0.0),
    };

    let bsp_data = Rc::new(BspData {
        planes: planes_rc.clone(),
        textures: textures.into_boxed_slice(),
        vertices: vertices.into_boxed_slice(),
        visibility: vis_data.into_boxed_slice(),
        render_nodes: render_nodes.into_boxed_slice(),
        texinfo: texinfo.into_boxed_slice(),
        faces: faces.into_boxed_slice(),
        lightmaps: lightmaps.into_boxed_slice(),
        hulls: [hull_0, hull_1, hull_2],
        leaves: leaves.into_boxed_slice(),
        facelist: facelist.into_boxed_slice(),
        edges: edges.into_boxed_slice(),
        edgelist: edgelist.into_boxed_slice(),
    });

    reader.seek(SeekFrom::Start(model_lump.offset))?;

    let mut total_leaf_count = 0;
    let mut brush_models = Vec::with_capacity(model_count);
    for i in 0..model_count {
        // we spread the bounds out by 1 unit in all directions. not sure why, but the original
        // engine does this. see
        // https://github.com/id-Software/Quake/blob/master/WinQuake/gl_model.c#L592
        let min = Vector3::new(
            reader.read_f32::<LittleEndian>()? - 1.0,
            reader.read_f32::<LittleEndian>()? - 1.0,
            reader.read_f32::<LittleEndian>()? - 1.0,
        );

        debug!("model[{}].min = {:?}", i, min);

        let max = Vector3::new(
            reader.read_f32::<LittleEndian>()? + 1.0,
            reader.read_f32::<LittleEndian>()? + 1.0,
            reader.read_f32::<LittleEndian>()? + 1.0,
        );

        debug!("model[{}].max = {:?}", i, max);

        let origin = Vector3::new(
            reader.read_f32::<LittleEndian>()?,
            reader.read_f32::<LittleEndian>()?,
            reader.read_f32::<LittleEndian>()?,
        );

        debug!("model[{}].origin = {:?}", i, max);

        let mut collision_node_ids = [0; MAX_HULLS];
        for i in 0..collision_node_ids.len() {
            collision_node_ids[i] = match reader.read_i32::<LittleEndian>()? {
                r if r < 0 => bail!("Invalid collision tree root node"),
                r => r as usize,
            };
        }

        // throw away the last collision node ID -- BSP files make room for 4 collision hulls but
        // only 3 are ever used.
        reader.read_i32::<LittleEndian>()?;

        debug!("model[{}].headnodes = {:?}", i, collision_node_ids);

        let leaf_id = total_leaf_count;
        debug!("model[{}].leaf_id = {:?}", i, leaf_id);

        let leaf_count = match reader.read_i32::<LittleEndian>()? {
            x if x < 0 => bail!("Invalid leaf count"),
            x => x as usize,
        };

        total_leaf_count += leaf_count;

        debug!("model[{}].leaf_count = {:?}", i, leaf_count);

        let face_id = match reader.read_i32::<LittleEndian>()? {
            x if x < 0 => bail!("Invalid face id"),
            x => x as usize,
        };

        let face_count = match reader.read_i32::<LittleEndian>()? {
            x if x < 0 => bail!("Invalid face count"),
            x => x as usize,
        };

        let mut collision_node_counts = [0; MAX_HULLS];
        for i in 0..collision_node_counts.len() {
            collision_node_counts[i] = collision_node_count - collision_node_ids[i];
        }

        brush_models.push(BspModel {
            bsp_data: bsp_data.clone(),
            min,
            max,
            origin,
            collision_node_ids,
            collision_node_counts,
            leaf_id,
            leaf_count,
            face_id,
            face_count,
        });
    }

    check_alignment(&mut reader, model_lump.offset + model_lump.size as u64)?;

    let models = brush_models
        .into_iter()
        .enumerate()
        .map(|(i, bmodel)| Model::from_brush_model(format!("*{}", i), bmodel))
        .collect();

    Ok((models, ent_string))
}
