// Copyright © 2017 Cormac O'Brien
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

use bsp::BspCollisionHull;
use bsp::BspCollisionNode;
use bsp::BspCollisionNodeChild;
use bsp::BspData;
use bsp::BspEdge;
use bsp::BspEdgeDirection;
use bsp::BspEdgeIndex;
use bsp::BspError;
use bsp::BspFace;
use bsp::BspFaceSide;
use bsp::BspLeaf;
use bsp::BspLeafContents;
use bsp::BspModel;
use bsp::BspPlane;
use bsp::BspPlaneAxis;
use bsp::BspRenderNode;
use bsp::BspRenderNodeChild;
use bsp::BspTexInfo;
use bsp::BspTexture;
use bsp::BspTextureAnimation;
use bsp::MAX_HULLS;
use bsp::MAX_LIGHTSTYLES;
use bsp::MIPLEVELS;
use model::Model;

use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use chrono::Duration;
use cgmath::Vector3;
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
    fn from_i32s(offset: i32, size: i32) -> Result<BspLump, BspError> {
        if offset < 0 {
            return Err(BspError::with_msg("Lump offset less than zero"));
        }

        if size < 0 {
            return Err(BspError::with_msg("Lump size less than zero"));
        }

        Ok(BspLump {
            offset: offset as u64,
            size: size as usize,
        })
    }
}


pub fn load(data: &[u8]) -> Result<(Vec<Model>, String), BspError> {
    let mut reader = BufReader::new(Cursor::new(data));

    let version = reader.read_i32::<LittleEndian>()?;
    if version != VERSION {
        error!(
            "Bad version number (found {}, should be {})",
            version,
            VERSION
        );
        return Err(BspError::with_msg("Bad version number"));
    }

    let mut lumps = Vec::with_capacity(BspLumpId::Count as usize);
    for l in 0..(BspLumpId::Count as usize) {
        let offset = match reader.read_i32::<LittleEndian>()? {
            o if o < 0 => return Err(BspError::Other(format!("Invalid lump offset of {}", o))),
            o => o,
        };

        let size = match reader.read_i32::<LittleEndian>()? {
            o if o < 0 => return Err(BspError::Other(format!("Invalid lump size of {}", o))),
            o => o,
        };

        debug!(
            "{: <14} Offset = 0x{:>08x} | Size = 0x{:>08x}",
            format!("{:?}:", BspLumpId::from_usize(l).unwrap()),
            offset,
            size
        );

        lumps.push(BspLump::from_i32s(offset, size).expect(
            "Failed to read lump",
        ));
    }

    let ent_lump = &lumps[BspLumpId::Entities as usize];
    reader.seek(SeekFrom::Start(ent_lump.offset))?;
    let mut ent_data = Vec::with_capacity(MAX_ENTSTRING);
    reader.read_until(0x00, &mut ent_data)?;
    if ent_data.len() > MAX_ENTSTRING {
        return Err(BspError::with_msg("Entity data exceeds MAX_ENTSTRING"));
    }
    let ent_string = String::from_utf8(ent_data).expect("Failed to create string from entity data");

    if reader.seek(SeekFrom::Current(0))? !=
        reader.seek(SeekFrom::Start(
            ent_lump.offset + ent_lump.size as u64,
        ))?
    {
        return Err(BspError::with_msg("BSP read data misaligned"));
    }

    let plane_lump = &lumps[BspLumpId::Planes as usize];
    reader.seek(SeekFrom::Start(plane_lump.offset))?;
    if plane_lump.size % PLANE_SIZE != 0 {
        return Err(BspError::with_msg(
            "BSP plane lump size not a multiple of lump size",
        ));
    }
    let plane_count = plane_lump.size / PLANE_SIZE;
    if plane_count > MAX_PLANES {
        return Err(BspError::with_msg("Plane count exceeds MAX_PLANES"));
    }
    let mut planes = Vec::with_capacity(plane_count);
    for _ in 0..plane_count {
        planes.push(BspPlane {
            normal: Vector3::new(
                reader.read_f32::<LittleEndian>()?,
                reader.read_f32::<LittleEndian>()?,
                reader.read_f32::<LittleEndian>()?,
            ),
            dist: reader.read_f32::<LittleEndian>()?,
            axis: BspPlaneAxis::from_i32(reader.read_i32::<LittleEndian>()?),
        });
    }

    let planes_rc = Rc::new(planes.into_boxed_slice());

    if reader.seek(SeekFrom::Current(0))? !=
        reader.seek(SeekFrom::Start(
            plane_lump.offset + plane_lump.size as u64,
        ))?
    {
        return Err(BspError::with_msg("BSP read data misaligned"));
    }

    let tex_lump = &lumps[BspLumpId::Textures as usize];
    reader.seek(SeekFrom::Start(tex_lump.offset))?;
    let tex_count = reader.read_i32::<LittleEndian>()?;
    if tex_count < 0 || tex_count as usize > MAX_TEXTURES {
        return Err(BspError::with_msg("Invalid texture count"));
    }
    let tex_count = tex_count as usize;
    let mut tex_offsets = Vec::with_capacity(tex_count);
    for _ in 0..tex_count {
        let ofs = reader.read_i32::<LittleEndian>()?;

        tex_offsets.push(match ofs {
            o if o < -1 => {
                return Err(BspError::with_msg(
                    format!("negative texture offset ({})", ofs),
                ))
            }
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

        reader.seek(
            SeekFrom::Start(tex_lump.offset + tex_ofs as u64),
        )?;
        let mut tex_name_bytes = [0u8; TEX_NAME_MAX];
        reader.read(&mut tex_name_bytes)?;
        let len = tex_name_bytes
            .iter()
            .enumerate()
            .find(|&item| item.1 == &0)
            .unwrap_or((TEX_NAME_MAX, &0))
            .0;
        let tex_name = String::from_utf8(tex_name_bytes[..len].to_vec()).unwrap();

        debug!(
            "Texture {id:>width$}: {name}",
            id = t,
            width = (tex_count as f32).log(10.0) as usize,
            name = tex_name
        );

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
            let offset = tex_lump.offset + (tex_ofs + mip_offsets[m]) as u64;
            reader.seek(SeekFrom::Start(offset))?;
            (&mut reader).take(mipmap_size as u64).read_to_end(
                &mut mipmaps[m],
            )?;
        }

        textures.push(BspTexture {
            name: tex_name,
            width: width,
            height: height,
            mipmaps: mipmaps,
            animation: None,
        })
    }

    if reader.seek(SeekFrom::Current(0))? !=
        reader.seek(SeekFrom::Start(
            tex_lump.offset + tex_lump.size as u64,
        ))?
    {
        return Err(BspError::with_msg("BSP read data misaligned"));
    }

    debug!("Sequencing textures");
    for t in 0..textures.len() {
        if !textures[t].name.starts_with("+") || textures[t].animation.is_some() {
            continue;
        }

        debug!("Sequencing texture {}", textures[t].name);

        let mut anim1 = [None; MAX_TEXTURE_FRAMES];
        let mut anim2 = [None; MAX_TEXTURE_FRAMES];
        let mut anim1_len = 0;
        let mut anim2_len = 0;

        let mut frame_char = textures[t].name.chars().nth(1).expect(
            "Invalid texture name",
        ) as usize;

        match frame_char {
            ASCII_0...ASCII_9 => {
                anim1_len = frame_char - ASCII_0;
                anim2_len = 0;
                anim1[anim1_len] = Some(t);
                anim1_len += 1;
            }

            ASCII_CAPITAL_A...ASCII_CAPITAL_J |
            ASCII_SMALL_A...ASCII_SMALL_J => {
                if frame_char >= ASCII_SMALL_A && frame_char <= ASCII_SMALL_J {
                    frame_char -= ASCII_SMALL_A - ASCII_CAPITAL_A;
                }
                anim2_len = frame_char - ASCII_CAPITAL_A;
                anim1_len = 0;
                anim2[anim2_len] = Some(t);
                anim2_len += 1;
            }

            _ => {
                return Err(BspError::with_msg(format!(
                    "Invalid texture frame specifier: U+{:x}",
                    frame_char
                )))
            }
        }

        for t2 in t + 1..textures.len() {
            // check if this texture has the same base name
            if !textures[t2].name.starts_with("+") ||
                textures[t2].name[2..] != textures[t].name[2..]
            {
                continue;
            }

            let mut frame_n_char = textures[t2].name.chars().nth(1).expect(
                "Invalid texture name",
            ) as usize;

            match frame_n_char {
                ASCII_0...ASCII_9 => {
                    frame_n_char -= ASCII_0;
                    anim1[frame_n_char] = Some(t2);
                    if frame_n_char + 1 > anim1_len {
                        anim1_len = frame_n_char + 1;
                    }
                }

                ASCII_CAPITAL_A...ASCII_CAPITAL_J |
                ASCII_SMALL_A...ASCII_SMALL_J => {
                    if frame_n_char >= ASCII_SMALL_A && frame_n_char <= ASCII_SMALL_J {
                        frame_n_char -= ASCII_SMALL_A - ASCII_CAPITAL_A;
                    }
                    frame_n_char -= ASCII_CAPITAL_A;
                    anim2[frame_n_char] = Some(t2);
                    if frame_n_char + 1 > anim2_len {
                        anim2_len += 1;
                    }
                }

                _ => {
                    return Err(BspError::with_msg(format!(
                        "Invalid texture frame specifier: U+{:x}",
                        frame_n_char
                    )))
                }
            }
        }

        for frame in 0..anim1_len {
            let mut tex2 = match anim1[frame] {
                Some(t2) => t2,
                None => {
                    return Err(BspError::with_msg(
                        format!("Missing frame {} of {}", frame, textures[t].name),
                    ))
                }
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
                None => {
                    return Err(BspError::with_msg(
                        format!("Missing frame {} of {}", frame, textures[t].name),
                    ))
                }
            };

            textures[tex2].animation = Some(BspTextureAnimation {
                sequence_duration: Duration::milliseconds(TEXTURE_FRAME_LEN_MS * anim2_len as i64),
                time_start: Duration::milliseconds(TEXTURE_FRAME_LEN_MS * frame as i64),
                time_end: Duration::milliseconds(TEXTURE_FRAME_LEN_MS * (frame as i64 + 1)),
                next: anim2[(frame + 1) % anim2_len].unwrap(),
            });
        }
    }

    let vert_lump = &lumps[BspLumpId::Vertices as usize];
    reader.seek(SeekFrom::Start(vert_lump.offset))?;
    if vert_lump.size % VERTEX_SIZE != 0 {
        return Err(BspError::with_msg(
            "BSP vertex lump size not a multiple of vertex size",
        ));
    }
    let vert_count = vert_lump.size / VERTEX_SIZE;
    if vert_count > MAX_VERTICES {
        return Err(BspError::with_msg("Vertex count exceeds MAX_VERTICES"));
    }
    let mut vertices = Vec::with_capacity(vert_count);
    for _ in 0..vert_count {
        vertices.push(Vector3::new(
            reader.read_f32::<LittleEndian>()?,
            reader.read_f32::<LittleEndian>()?,
            reader.read_f32::<LittleEndian>()?,
        ));
    }
    if reader.seek(SeekFrom::Current(0))? !=
        reader.seek(SeekFrom::Start(
            vert_lump.offset + vert_lump.size as u64,
        ))?
    {
        return Err(BspError::with_msg("BSP read data misaligned"));
    }

    let vis_lump = &lumps[BspLumpId::Visibility as usize];
    reader.seek(SeekFrom::Start(vis_lump.offset))?;
    if vis_lump.size > MAX_VISLIST {
        return Err(BspError::with_msg(
            "Visibility data size exceeds MAX_VISLIST",
        ));
    }
    let mut vis_data = Vec::with_capacity(vis_lump.size);
    (&mut reader).take(vis_lump.size as u64).read_to_end(
        &mut vis_data,
    )?;
    if reader.seek(SeekFrom::Current(0))? !=
        reader.seek(SeekFrom::Start(
            vis_lump.offset + vis_lump.size as u64,
        ))?
    {
        return Err(BspError::with_msg("BSP read data misaligned"));
    }

    let render_node_lump = &lumps[BspLumpId::RenderNodes as usize];
    reader.seek(SeekFrom::Start(render_node_lump.offset))?;
    if render_node_lump.size % RENDER_NODE_SIZE != 0 {
        return Err(BspError::with_msg(
            "BSP lump node size not a multiple of node size",
        ));
    }
    let render_node_count = render_node_lump.size / RENDER_NODE_SIZE;
    if render_node_count > MAX_RENDER_NODES {
        return Err(BspError::with_msg("Render node count exceeds MAX_RENDER_NODES"));
    }
    debug!("Render node count = {}", render_node_count);
    let mut render_nodes = Vec::with_capacity(render_node_count);
    for _ in 0..render_node_count {
        let plane_id = reader.read_i32::<LittleEndian>()?;
        if plane_id < 0 {
            return Err(BspError::with_msg("Invalid plane id"));
        }

        // If the child ID is positive, it points to another internal node. If it is negative, it
        // points to a leaf node, but we have to negate it *and subtract 1* because ID -1
        // corresponds to leaf 0.

        let front = match reader.read_i16::<LittleEndian>()? {
            f if f < 0 => BspRenderNodeChild::Leaf(-f as usize - 1),
            f => BspRenderNodeChild::Node(f as usize),
        };

        let back = match reader.read_i16::<LittleEndian>()? {
            b if b < 0 => BspRenderNodeChild::Leaf(-b as usize - 1),
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
            return Err(BspError::with_msg("Invalid face id"));
        }

        let face_count = reader.read_u16::<LittleEndian>()?;
        if face_count as usize > MAX_FACES {
            return Err(BspError::with_msg("Invalid face count"));
        }

        render_nodes.push(BspRenderNode {
            plane_id: plane_id as usize,
            children: [front, back],
            min: min,
            max: max,
            face_id: face_id as usize,
            face_count: face_count as usize,
        });
    }
    if reader.seek(SeekFrom::Current(0))? !=
        reader.seek(SeekFrom::Start(
            render_node_lump.offset + render_node_lump.size as u64,
        ))?
    {
        return Err(BspError::with_msg("BSP read data misaligned"));
    }

    let texinfo_lump = &lumps[BspLumpId::TextureInfo as usize];
    reader.seek(SeekFrom::Start(texinfo_lump.offset))?;
    if texinfo_lump.size % TEXINFO_SIZE != 0 {
        return Err(BspError::with_msg(
            "BSP texinfo lump size not a multiple of texinfo size",
        ));
    }
    let texinfo_count = texinfo_lump.size / TEXINFO_SIZE;
    let mut texinfo = Vec::with_capacity(texinfo_count);
    for _ in 0..texinfo_count {
        texinfo.push(BspTexInfo {
            s_vector: Vector3::new(
                reader.read_f32::<LittleEndian>()?,
                reader.read_f32::<LittleEndian>()?,
                reader.read_f32::<LittleEndian>()?,
            ),
            s_offset: reader.read_f32::<LittleEndian>()?,
            t_vector: Vector3::new(
                reader.read_f32::<LittleEndian>()?,
                reader.read_f32::<LittleEndian>()?,
                reader.read_f32::<LittleEndian>()?,
            ),
            t_offset: reader.read_f32::<LittleEndian>()?,

            tex_id: match reader.read_i32::<LittleEndian>()? {
                t if t < 0 || t as usize > tex_count => {
                    return Err(BspError::with_msg("Invalid texture ID"))
                }
                t => t as usize,
            },
            animated: match reader.read_i32::<LittleEndian>()? {
                0 => false,
                1 => true,
                _ => return Err(BspError::with_msg("Invalid texture flags")),
            },
        });
    }
    if reader.seek(SeekFrom::Current(0))? !=
        reader.seek(SeekFrom::Start(
            texinfo_lump.offset + texinfo_lump.size as u64,
        ))?
    {
        return Err(BspError::with_msg("BSP read data misaligned"));
    }

    let face_lump = &lumps[BspLumpId::Faces as usize];
    reader.seek(SeekFrom::Start(face_lump.offset))?;
    if face_lump.size % FACE_SIZE != 0 {
        return Err(BspError::with_msg(
            "BSP face lump size not a multiple of face size",
        ));
    }
    let face_count = face_lump.size / FACE_SIZE;
    let mut faces = Vec::with_capacity(face_count);
    for _ in 0..face_count {
        let plane_id = reader.read_i16::<LittleEndian>()?;
        if plane_id < 0 || plane_id as usize > plane_count {
            return Err(BspError::with_msg("Invalid plane count"));
        }

        let side = match reader.read_i16::<LittleEndian>()? {
            0 => BspFaceSide::Front,
            1 => BspFaceSide::Back,
            _ => return Err(BspError::with_msg("Invalid face side")),
        };

        let edge_id = reader.read_i32::<LittleEndian>()?;
        if edge_id < 0 {
            return Err(BspError::with_msg("Invalid edge ID"));
        }

        let edge_count = reader.read_i16::<LittleEndian>()?;
        if edge_count < 3 {
            return Err(BspError::with_msg("Invalid edge count"));
        }

        let texinfo_id = reader.read_i16::<LittleEndian>()?;
        if texinfo_id < 0 || texinfo_id as usize > texinfo_count {
            return Err(BspError::with_msg("Invalid texinfo ID"));
        }

        let mut light_styles = [0; MAX_LIGHTSTYLES];
        for i in 0..light_styles.len() {
            light_styles[i] = reader.read_u8()?;
        }

        let lightmap_id = match reader.read_i32::<LittleEndian>()? {
            o if o < -1 => return Err(BspError::with_msg("Invalid lightmap offset")),
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
        });
    }
    if reader.seek(SeekFrom::Current(0))? !=
        reader.seek(SeekFrom::Start(
            face_lump.offset + face_lump.size as u64,
        ))?
    {
        return Err(BspError::with_msg("BSP read data misaligned"));
    }

    let lightmap_lump = &lumps[BspLumpId::Lightmaps as usize];
    reader.seek(SeekFrom::Start(lightmap_lump.offset))?;
    let mut lightmaps = Vec::with_capacity(lightmap_lump.size);
    (&mut reader).take(lightmap_lump.size as u64).read_to_end(
        &mut lightmaps,
    )?;
    if reader.seek(SeekFrom::Current(0))? !=
        reader.seek(SeekFrom::Start(
            lightmap_lump.offset + lightmap_lump.size as u64,
        ))?
    {
        return Err(BspError::with_msg("BSP read data misaligned"));
    }

    let collision_node_lump = &lumps[BspLumpId::CollisionNodes as usize];
    reader.seek(SeekFrom::Start(collision_node_lump.offset))?;
    if collision_node_lump.size % COLLISION_NODE_SIZE != 0 {
        return Err(BspError::with_msg(
            "BSP collision_node lump size not a multiple of collision_node size",
        ));
    }

    let collision_node_count = collision_node_lump.size / COLLISION_NODE_SIZE;
    if collision_node_count > MAX_COLLISION_NODES {
        return Err(BspError::with_msg(format!(
            "Collision node count ({}) exceeds MAX_COLLISION_NODES ({})",
            collision_node_count,
            MAX_COLLISION_NODES
        )));
    }

    let mut collision_nodes = Vec::with_capacity(collision_node_count);
    for _ in 0..collision_node_count {
        let plane_id = match reader.read_i32::<LittleEndian>()? {
            x if x < 0 => return Err(BspError::with_msg("Invalid plane id")),
            x => x as usize,
        };

        let front = match reader.read_i16::<LittleEndian>()? {
            x if x < 0 => match BspLeafContents::from_i16(-x) {
                Some(c) => BspCollisionNodeChild::Contents(c),
                None => return Err(BspError::with_msg(format!("Invalid leaf contents ({})", -x))),
            }
            x => BspCollisionNodeChild::Node(x as usize),
        };

        let back = match reader.read_i16::<LittleEndian>()? {
            x if x < 0 => match BspLeafContents::from_i16(-x) {
                Some(c) => BspCollisionNodeChild::Contents(c),
                None => return Err(BspError::with_msg(format!("Invalid leaf contents ({})", -x))),
            }
            x => BspCollisionNodeChild::Node(x as usize),
        };

        collision_nodes.push(BspCollisionNode {
            plane_id,
            children: [front, back]
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

    if reader.seek(SeekFrom::Current(0))? !=
        reader.seek(SeekFrom::Start(
            collision_node_lump.offset +
                 collision_node_lump.size as u64,
        ))?
    {
        return Err(BspError::with_msg("BSP read data misaligned"));
    }

    let leaf_lump = &lumps[BspLumpId::Leaves as usize];
    reader.seek(SeekFrom::Start(leaf_lump.offset))?;
    if leaf_lump.size % LEAF_SIZE != 0 {
        return Err(BspError::with_msg(
            "BSP leaf lump size not a multiple of leaf size",
        ));
    }

    let leaf_count = leaf_lump.size / LEAF_SIZE;
    if leaf_count > MAX_LEAVES {
        return Err(BspError::with_msg("Leaf count exceeds MAX_LEAVES"));
    }

    let mut leaves = Vec::with_capacity(leaf_count);
    for _ in 0..leaf_count {
        // note the negation here (the constants are negative in the original engine to differentiate
        // them from plane IDs)
        let contents_id = -reader.read_i32::<LittleEndian>()?;

        let contents = match BspLeafContents::from_i32(contents_id) {
            Some(c) => c,
            None => return Err(BspError::with_msg(format!("Invalid leaf contents ({})", contents_id))),
        };

        let vis_offset = match reader.read_i32::<LittleEndian>()? {
            x if x < -1 => return Err(BspError::with_msg("Invalid visibility data offset")),
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

        let face_id = reader.read_u16::<LittleEndian>()? as usize;
        let face_count = reader.read_u16::<LittleEndian>()? as usize;
        let mut sounds = [0u8; NUM_AMBIENTS];
        reader.read(&mut sounds)?;
        leaves.push(BspLeaf {
            contents: contents,
            vis_offset: vis_offset,
            min: min,
            max: max,
            face_id: face_id,
            face_count: face_count,
            sounds: sounds,
        });
    }
    if reader.seek(SeekFrom::Current(0))? !=
        reader.seek(SeekFrom::Start(
            leaf_lump.offset + leaf_lump.size as u64,
        ))?
    {
        return Err(BspError::with_msg("BSP read data misaligned"));
    }

    let facelist_lump = &lumps[BspLumpId::FaceList as usize];
    reader.seek(SeekFrom::Start(facelist_lump.offset))?;
    if facelist_lump.size % FACELIST_SIZE != 0 {
        return Err(BspError::with_msg(
            "BSP facelist lump size not a multiple of facelist entry size",
        ));
    }
    let facelist_count = facelist_lump.size / FACELIST_SIZE;
    let mut facelist = Vec::with_capacity(facelist_count);
    for _ in 0..facelist_count {
        facelist.push(reader.read_u16::<LittleEndian>()? as usize);
    }
    if reader.seek(SeekFrom::Current(0))? !=
        reader.seek(SeekFrom::Start(
            facelist_lump.offset + facelist_lump.size as u64,
        ))?
    {
        return Err(BspError::with_msg("BSP read data misaligned"));
    }

    let edge_lump = &lumps[BspLumpId::Edges as usize];
    reader.seek(SeekFrom::Start(edge_lump.offset))?;
    if edge_lump.size % EDGE_SIZE != 0 {
        return Err(BspError::with_msg(
            "BSP edge lump size not a multiple of edge size",
        ));
    }
    let edge_count = edge_lump.size / EDGE_SIZE;
    if edge_count > MAX_EDGES {
        return Err(BspError::with_msg("Edge count exceeds MAX_EDGES"));
    }
    let mut edges = Vec::with_capacity(edge_count);
    for _ in 0..edge_count {
        edges.push(BspEdge {
            vertex_ids: [
                reader.read_u16::<LittleEndian>()?,
                reader.read_u16::<LittleEndian>()?,
            ],
        });
    }
    if reader.seek(SeekFrom::Current(0))? !=
        reader.seek(SeekFrom::Start(
            edge_lump.offset + edge_lump.size as u64,
        ))?
    {
        return Err(BspError::with_msg("BSP read data misaligned"));
    }

    let edgelist_lump = &lumps[BspLumpId::EdgeList as usize];
    reader.seek(SeekFrom::Start(edgelist_lump.offset))?;
    if edgelist_lump.size % EDGELIST_SIZE != 0 {
        return Err(BspError::with_msg(
            "BSP edgelist lump size not a multiple of edgelist entry size",
        ));
    }
    let edgelist_count = edgelist_lump.size / EDGELIST_SIZE;
    if edgelist_count > MAX_EDGELIST {
        return Err(BspError::with_msg("Edge list count exceeds MAX_EDGELIST"));
    }
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

            x => return Err(BspError::with_msg(format!("Invalid edge index {}", x))),
        });
    }
    if reader.seek(SeekFrom::Current(0))? !=
        reader.seek(SeekFrom::Start(
            edgelist_lump.offset + edgelist_lump.size as u64,
        ))?
    {
        return Err(BspError::with_msg("BSP read data misaligned"));
    }

    // see Mod_MakeHull0,
    // https://github.com/id-Software/Quake/blob/master/WinQuake/gl_model.c#L1001-L1031
    //
    // This essentially duplicates the render nodes into a tree of collision nodes.
    let mut render_as_collision_nodes = Vec::with_capacity(render_nodes.len());
    for i in 0..render_nodes.len() {
        render_as_collision_nodes.push(BspCollisionNode {
            plane_id: render_nodes[i].plane_id,
            children: [match render_nodes[i].children[0] {
                BspRenderNodeChild::Node(n) => BspCollisionNodeChild::Node(n),
                BspRenderNodeChild::Leaf(l) => BspCollisionNodeChild::Contents(leaves[l].contents),
            },
            match render_nodes[i].children[1] {
                BspRenderNodeChild::Node(n) => BspCollisionNodeChild::Node(n),
                BspRenderNodeChild::Leaf(l) => BspCollisionNodeChild::Contents(leaves[l].contents),
            }]
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

    let model_lump = &lumps[BspLumpId::Models as usize];
    reader.seek(SeekFrom::Start(model_lump.offset))?;
    if model_lump.size % MODEL_SIZE != 0 {
        return Err(BspError::with_msg(
            "BSP model lump size not a multiple of model size",
        ));
    }
    let model_count = model_lump.size / MODEL_SIZE;

    if model_count < 1 {
        return Err(BspError::with_msg(
            "No brush models (need at least 1 for worldmodel)",
        ));
    }

    if model_count > MAX_MODELS {
        return Err(BspError::with_msg("Model count exceeds MAX_MODELS"));
    }

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
                r if r < 0 => return Err(BspError::with_msg("Invalid collision tree root node")),
                r => r as usize,
            };
        }

        // throw away the last collision node ID -- BSP files make room for 4 collision hulls but
        // only 3 are ever used.
        reader.read_i32::<LittleEndian>()?;

        debug!("model[{}].headnodes = {:?}", i, collision_node_ids);

        let leaf_count = match reader.read_i32::<LittleEndian>()? {
            x if x < 0 => return Err(BspError::with_msg("Invalid leaf count")),
            x => x as usize,
        };

        debug!("model[{}].leaf_count = {:?}", i, leaf_count);

        let face_id = match reader.read_i32::<LittleEndian>()? {
            x if x < 0 => return Err(BspError::with_msg("Invalid face id")),
            x => x as usize,
        };

        let face_count = match reader.read_i32::<LittleEndian>()? {
            x if x < 0 => return Err(BspError::with_msg("Invalid face count")),
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
            leaf_count,
            face_id,
            face_count,
        });
    }

    if reader.seek(SeekFrom::Current(0))? !=
        reader.seek(SeekFrom::Start(
            model_lump.offset + model_lump.size as u64,
        ))?
    {
        return Err(BspError::with_msg("BSP read data misaligned"));
    }

    let models = brush_models.into_iter()
        .enumerate()
        .map(|(i, bmodel)| Model::from_brush_model(format!("*{}", i), bmodel))
        .collect();

    Ok((models, ent_string))
}
