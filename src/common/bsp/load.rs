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

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    mem::size_of,
    rc::Rc,
};

use crate::common::{
    bsp::{
        BspCollisionHull, BspCollisionNode, BspCollisionNodeChild, BspData, BspEdge,
        BspEdgeDirection, BspEdgeIndex, BspFace, BspFaceSide, BspLeaf, BspLeafContents, BspModel,
        BspRenderNode, BspRenderNodeChild, BspTexInfo, BspTexture, MAX_HULLS, MAX_LIGHTSTYLES,
        MIPLEVELS,
    },
    math::{Axis, Hyperplane},
    model::Model,
    util::read_f32_3,
};

use super::{BspTextureFrame, BspTextureKind};
use byteorder::{LittleEndian, ReadBytesExt};
use cgmath::{InnerSpace, Vector3};
use chrono::Duration;
use failure::ResultExt as _;
use num::FromPrimitive;
use thiserror::Error;

const VERSION: i32 = 29;

pub const MAX_MODELS: usize = 256;
const MAX_LEAVES: usize = 32767;

const MAX_ENTSTRING: usize = 65536;
const MAX_PLANES: usize = 8192;
const MAX_RENDER_NODES: usize = 32767;
const MAX_COLLISION_NODES: usize = 32767;
const MAX_VERTICES: usize = 65535;
const MAX_FACES: usize = 65535;
const _MAX_MARKTEXINFO: usize = 65535;
const _MAX_TEXINFO: usize = 4096;
const MAX_EDGES: usize = 256000;
const MAX_EDGELIST: usize = 512000;
const MAX_TEXTURES: usize = 0x200000;
const _MAX_LIGHTMAP: usize = 0x100000;
const MAX_VISLIST: usize = 0x100000;

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

#[derive(Error, Debug)]
pub enum BspFileError {
    #[error("I/O error")]
    Io(#[from] std::io::Error),
    #[error("unsupported BSP format version (expected {}, found {0})", VERSION)]
    UnsupportedVersion(i32),
    #[error("negative BSP file section offset: {0}")]
    NegativeSectionOffset(i32),
    #[error("negative BSP file section size: {0}")]
    NegativeSectionSize(i32),
    #[error(
        "invalid BSP file section size: section {section:?} size is {size}, must be multiple of {}",
        section.element_size(),
    )]
    InvalidSectionSize {
        section: BspFileSectionId,
        size: usize,
    },
    #[error("invalid BSP texture frame specifier: {0}")]
    InvalidTextureFrameSpecifier(String),
    #[error("texture has primary animation with 0 frames: {0}")]
    EmptyPrimaryAnimation(String),
}

#[derive(Copy, Clone, Debug)]
struct BspFileSection {
    offset: u64,
    size: usize,
}

impl BspFileSection {
    fn read_from<R>(reader: &mut R) -> Result<BspFileSection, BspFileError>
    where
        R: ReadBytesExt,
    {
        let offset = match reader.read_i32::<LittleEndian>()? {
            ofs if ofs < 0 => Err(BspFileError::NegativeSectionOffset(ofs)),
            ofs => Ok(ofs as u64),
        }?;

        let size = match reader.read_i32::<LittleEndian>()? {
            sz if sz < 0 => Err(BspFileError::NegativeSectionSize(sz)),
            sz => Ok(sz as usize),
        }?;

        Ok(BspFileSection { offset, size })
    }
}

const SECTION_COUNT: usize = 15;
#[derive(Debug, FromPrimitive)]
pub enum BspFileSectionId {
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
}

const PLANE_SIZE: usize = 20;
const RENDER_NODE_SIZE: usize = 24;
const LEAF_SIZE: usize = 28;
const TEXTURE_INFO_SIZE: usize = 40;
const FACE_SIZE: usize = 20;
const COLLISION_NODE_SIZE: usize = 8;
const FACELIST_SIZE: usize = 2;
const EDGE_SIZE: usize = 4;
const EDGELIST_SIZE: usize = 4;
const MODEL_SIZE: usize = 64;
const VERTEX_SIZE: usize = 12;

impl BspFileSectionId {
    // the size on disk of one element of a BSP file section.
    fn element_size(&self) -> usize {
        use BspFileSectionId::*;
        match self {
            Entities => size_of::<u8>(),
            Planes => PLANE_SIZE,
            Textures => size_of::<u8>(),
            Vertices => VERTEX_SIZE,
            Visibility => size_of::<u8>(),
            RenderNodes => RENDER_NODE_SIZE,
            TextureInfo => TEXTURE_INFO_SIZE,
            Faces => FACE_SIZE,
            Lightmaps => size_of::<u8>(),
            CollisionNodes => COLLISION_NODE_SIZE,
            Leaves => LEAF_SIZE,
            FaceList => FACELIST_SIZE,
            Edges => EDGE_SIZE,
            EdgeList => EDGELIST_SIZE,
            Models => MODEL_SIZE,
        }
    }
}

struct BspFileTable {
    sections: [BspFileSection; SECTION_COUNT],
}

impl BspFileTable {
    fn read_from<R>(reader: &mut R) -> Result<BspFileTable, BspFileError>
    where
        R: ReadBytesExt,
    {
        let mut sections = [BspFileSection { offset: 0, size: 0 }; SECTION_COUNT];

        for (id, section) in sections.iter_mut().enumerate() {
            *section = BspFileSection::read_from(reader)?;
            let section_id = BspFileSectionId::from_usize(id).unwrap();
            if section.size % section_id.element_size() != 0 {
                Err(BspFileError::InvalidSectionSize {
                    section: section_id,
                    size: section.size,
                })?
            }
        }

        Ok(BspFileTable { sections })
    }

    fn section(&self, section_id: BspFileSectionId) -> BspFileSection {
        self.sections[section_id as usize]
    }

    fn check_end_position<S>(
        &self,
        seeker: &mut S,
        section_id: BspFileSectionId,
    ) -> Result<(), failure::Error>
    where
        S: Seek,
    {
        let section = self.section(section_id);
        ensure!(
            seeker.seek(SeekFrom::Current(0))?
                == seeker.seek(SeekFrom::Start(section.offset + section.size as u64))?,
            "BSP read misaligned"
        );

        Ok(())
    }
}

fn read_hyperplane<R>(reader: &mut R) -> Result<Hyperplane, failure::Error>
where
    R: ReadBytesExt,
{
    let normal: Vector3<f32> = read_f32_3(reader)?.into();
    let dist = reader.read_f32::<LittleEndian>()?;
    let plane = match Axis::from_i32(reader.read_i32::<LittleEndian>()?) {
        Some(ax) => match ax {
            Axis::X => Hyperplane::axis_x(dist),
            Axis::Y => Hyperplane::axis_y(dist),
            Axis::Z => Hyperplane::axis_z(dist),
        },
        None => Hyperplane::new(normal, dist),
    };

    Ok(plane)
}

#[derive(Debug)]
struct BspFileTexture {
    name: String,
    width: u32,
    height: u32,
    mipmaps: [Vec<u8>; MIPLEVELS],
}

// load a textures from the BSP file.
//
// converts the texture's name to all lowercase, including its frame specifier
// if it has one.
fn load_texture<R>(
    mut reader: &mut R,
    tex_section_ofs: u64,
    tex_ofs: u64,
) -> Result<BspFileTexture, failure::Error>
where
    R: ReadBytesExt + Seek,
{
    // convert texture name from NUL-terminated to str
    let mut tex_name_bytes = [0u8; TEX_NAME_MAX];
    reader.read_exact(&mut tex_name_bytes)?;
    let len = tex_name_bytes
        .iter()
        .enumerate()
        .find(|&item| item.1 == &0)
        .unwrap_or((TEX_NAME_MAX, &0))
        .0;
    let tex_name = String::from_utf8(tex_name_bytes[..len].to_vec())?.to_lowercase();

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
        let offset = tex_section_ofs + tex_ofs + mip_offsets[m] as u64;
        reader.seek(SeekFrom::Start(offset))?;
        (&mut reader)
            .take(mipmap_size as u64)
            .read_to_end(&mut mipmaps[m])?;
    }

    Ok(BspFileTexture {
        name: tex_name,
        width,
        height,
        mipmaps,
    })
}

fn load_render_node<R>(reader: &mut R) -> Result<BspRenderNode, failure::Error>
where
    R: ReadBytesExt,
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

    let min = read_i16_3(reader)?;
    let max = read_i16_3(reader)?;

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
        min,
        max,
        face_id: face_id as usize,
        face_count: face_count as usize,
    })
}

fn load_texinfo<R>(reader: &mut R, texture_count: usize) -> Result<BspTexInfo, failure::Error>
where
    R: ReadBytesExt,
{
    let s_vector = read_f32_3(reader)?.into();
    let s_offset = reader.read_f32::<LittleEndian>()?;
    let t_vector = read_f32_3(reader)?.into();
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
pub fn load<R>(data: R) -> Result<(Vec<Model>, String), failure::Error>
where
    R: Read + Seek,
{
    let mut reader = BufReader::new(data);

    let _version = match reader.read_i32::<LittleEndian>()? {
        VERSION => Ok(VERSION),
        other => Err(BspFileError::UnsupportedVersion(other)),
    }?;

    let table = BspFileTable::read_from(&mut reader)?;

    let ent_section = table.section(BspFileSectionId::Entities);
    let plane_section = table.section(BspFileSectionId::Planes);
    let tex_section = table.section(BspFileSectionId::Textures);
    let vert_section = table.section(BspFileSectionId::Vertices);
    let vis_section = table.section(BspFileSectionId::Visibility);
    let texinfo_section = table.section(BspFileSectionId::TextureInfo);
    let face_section = table.section(BspFileSectionId::Faces);
    let lightmap_section = table.section(BspFileSectionId::Lightmaps);
    let collision_node_section = table.section(BspFileSectionId::CollisionNodes);
    let leaf_section = table.section(BspFileSectionId::Leaves);
    let facelist_section = table.section(BspFileSectionId::FaceList);
    let edge_section = table.section(BspFileSectionId::Edges);
    let edgelist_section = table.section(BspFileSectionId::EdgeList);
    let model_section = table.section(BspFileSectionId::Models);
    let render_node_section = table.section(BspFileSectionId::RenderNodes);

    let plane_count = plane_section.size / PLANE_SIZE;
    let vert_count = vert_section.size / VERTEX_SIZE;
    let render_node_count = render_node_section.size / RENDER_NODE_SIZE;
    let texinfo_count = texinfo_section.size / TEXTURE_INFO_SIZE;
    let face_count = face_section.size / FACE_SIZE;
    let collision_node_count = collision_node_section.size / COLLISION_NODE_SIZE;
    let leaf_count = leaf_section.size / LEAF_SIZE;
    let facelist_count = facelist_section.size / FACELIST_SIZE;
    let edge_count = edge_section.size / EDGE_SIZE;
    let edgelist_count = edgelist_section.size / EDGELIST_SIZE;
    let model_count = model_section.size / MODEL_SIZE;

    // check limits
    ensure!(plane_count <= MAX_PLANES, "Plane count exceeds MAX_PLANES");
    ensure!(
        vert_count <= MAX_VERTICES,
        "Vertex count exceeds MAX_VERTICES"
    );
    ensure!(
        vis_section.size <= MAX_VISLIST,
        "Visibility data size exceeds MAX_VISLIST"
    );
    ensure!(
        render_node_count <= MAX_RENDER_NODES,
        "Render node count exceeds MAX_RENDER_NODES"
    );
    ensure!(
        collision_node_count <= MAX_COLLISION_NODES,
        "Collision node count exceeds MAX_COLLISION_NODES"
    );
    ensure!(leaf_count <= MAX_LEAVES, "Leaf count exceeds MAX_LEAVES");
    ensure!(edge_count <= MAX_EDGES, "Edge count exceeds MAX_EDGES");
    ensure!(
        edgelist_count <= MAX_EDGELIST,
        "Edge list count exceeds MAX_EDGELIST"
    );
    ensure!(
        model_count > 0,
        "No brush models (need at least 1 for worldmodel)"
    );
    ensure!(model_count <= MAX_MODELS, "Model count exceeds MAX_MODELS");

    reader.seek(SeekFrom::Start(ent_section.offset))?;
    let mut ent_data = Vec::with_capacity(MAX_ENTSTRING);
    reader.read_until(0x00, &mut ent_data)?;
    ensure!(
        ent_data.len() <= MAX_ENTSTRING,
        "Entity data exceeds MAX_ENTSTRING"
    );
    let ent_string =
        String::from_utf8(ent_data).context("Failed to create string from entity data")?;
    table.check_end_position(&mut reader, BspFileSectionId::Entities)?;

    // load planes
    reader.seek(SeekFrom::Start(plane_section.offset))?;
    let mut planes = Vec::with_capacity(plane_count);
    for _ in 0..plane_count {
        planes.push(read_hyperplane(&mut reader)?);
    }
    let planes_rc = Rc::new(planes.into_boxed_slice());

    table.check_end_position(&mut reader, BspFileSectionId::Planes)?;

    // load textures
    reader.seek(SeekFrom::Start(tex_section.offset))?;
    let tex_count = reader.read_i32::<LittleEndian>()?;
    ensure!(
        tex_count >= 0 && tex_count as usize <= MAX_TEXTURES,
        "Invalid texture count"
    );
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

    let mut file_textures = Vec::with_capacity(tex_count);
    for (id, tex_ofs) in tex_offsets.into_iter().enumerate() {
        match tex_ofs {
            Some(ofs) => {
                reader.seek(SeekFrom::Start(tex_section.offset + ofs as u64))?;
                let texture = load_texture(&mut reader, tex_section.offset as u64, ofs as u64)?;
                debug!(
                    "Texture {id:>width$}: {name}",
                    id = id,
                    width = (tex_count as f32).log(10.0) as usize,
                    name = texture.name,
                );

                file_textures.push(texture);
            }

            None => {
                file_textures.push(BspFileTexture {
                    name: String::new(),
                    width: 0,
                    height: 0,
                    mipmaps: [Vec::new(), Vec::new(), Vec::new(), Vec::new()],
                });
            }
        }
    }
    table.check_end_position(&mut reader, BspFileSectionId::Textures)?;

    struct BspFileTextureAnimations {
        primary: Vec<(usize, BspFileTexture)>,
        alternate: Vec<(usize, BspFileTexture)>,
    }

    // maps animated texture names to primary and alternate animations
    // e.g., for textures of the form +#slip, maps "slip" to the ids of
    // [+0slip, +1slip, ...] and [+aslip, +bslip, ...]
    let mut anim_file_textures: HashMap<String, BspFileTextureAnimations> = HashMap::new();

    // final texture array
    let mut textures = Vec::new();
    // mapping from texture ids on disk to texture ids in memory
    let mut texture_ids = Vec::new();

    // map file texture ids to actual texture ids
    let mut static_texture_ids = HashMap::new();
    let mut animated_texture_ids = HashMap::new();

    debug!("Sequencing textures");
    for (file_texture_id, file_texture) in file_textures.into_iter().enumerate() {
        // recognize textures of the form +[frame][stem], where:
        // - frame is in [0-9A-Za-z]
        // - stem is the remainder of the string
        match file_texture.name.strip_prefix("+") {
            Some(rest) => {
                let (frame, stem) = rest.split_at(1);

                debug!(
                    "Sequencing texture {}: {}",
                    file_texture_id, &file_texture.name
                );

                let anims =
                    anim_file_textures
                        .entry(stem.to_owned())
                        .or_insert(BspFileTextureAnimations {
                            primary: Vec::new(),
                            alternate: Vec::new(),
                        });

                match frame.chars().nth(0).unwrap() {
                    '0'..='9' => anims.primary.push((file_texture_id, file_texture)),
                    // guaranteed to be lowercase by load_texture
                    'a'..='j' => anims.alternate.push((file_texture_id, file_texture)),
                    _ => Err(BspFileError::InvalidTextureFrameSpecifier(
                        file_texture.name.clone(),
                    ))?,
                };
            }

            // if the string doesn't match, it's not animated, so add it as a static texture
            None => {
                let BspFileTexture {
                    name,
                    width,
                    height,
                    mipmaps,
                } = file_texture;

                let texture_id = textures.len();
                static_texture_ids.insert(file_texture_id, texture_id);

                textures.push(BspTexture {
                    name,
                    width,
                    height,
                    kind: BspTextureKind::Static(BspTextureFrame { mipmaps }),
                });
            }
        };
    }

    // sequence animated textures with the same stem
    for (
        name,
        BspFileTextureAnimations {
            primary: mut pri,
            alternate: mut alt,
        },
    ) in anim_file_textures.into_iter()
    {
        if pri.len() == 0 {
            Err(BspFileError::EmptyPrimaryAnimation(name.to_owned()))?;
        }

        // TODO: ensure one-to-one frame specifiers
        // sort names in ascending order to get the frames ordered correctly
        pri.sort_unstable_by(|(_, tex), (_, other)| tex.name.cmp(&other.name));

        // TODO: verify width and height?
        let width = pri[0].1.width;
        let height = pri[0].1.height;

        // texture id of each frame in the file
        let mut corresponding_file_ids = Vec::new();
        let mut primary = Vec::new();
        for (file_id, file_texture) in pri {
            debug!(
                "primary frame: id = {}, name = {}",
                file_id, file_texture.name
            );
            corresponding_file_ids.push(file_id);
            primary.push(BspTextureFrame {
                mipmaps: file_texture.mipmaps,
            });
        }

        let mut alt_corresp_file_ids = Vec::new();
        let alternate = match alt.len() {
            0 => None,
            _ => {
                alt.sort_unstable_by(|(_, tex), (_, other)| tex.name.cmp(&other.name));
                let mut alternate = Vec::new();
                for (file_id, file_texture) in alt {
                    alt_corresp_file_ids.push(file_id);
                    alternate.push(BspTextureFrame {
                        mipmaps: file_texture.mipmaps,
                    });
                }
                Some(alternate)
            }
        };

        // actual id of the animated texture
        let texture_id = textures.len();

        // update map to point other data to the right texture
        for id in corresponding_file_ids {
            debug!("map disk texture id {} to texture id {}", id, texture_id);
            animated_texture_ids.insert(id, texture_id);
        }

        for id in alt_corresp_file_ids {
            debug!("map disk texture id {} to texture id {}", id, texture_id);
            animated_texture_ids.insert(id, texture_id);
        }

        // push the sequenced texture
        textures.push(BspTexture {
            name: name.to_owned(),
            width,
            height,
            kind: BspTextureKind::Animated { primary, alternate },
        });
    }

    // build disk-to-memory texture id map
    for file_texture_id in 0..tex_count {
        texture_ids.push(if let Some(id) = static_texture_ids.get(&file_texture_id) {
            *id
        } else if let Some(id) = animated_texture_ids.get(&file_texture_id) {
            *id
        } else {
            panic!(
                "Texture sequencing failed: texture with id {} unaccounted for",
                file_texture_id
            );
        });
    }

    reader.seek(SeekFrom::Start(vert_section.offset))?;
    let mut vertices = Vec::with_capacity(vert_count);
    for _ in 0..vert_count {
        vertices.push(read_f32_3(&mut reader)?.into());
    }
    table.check_end_position(&mut reader, BspFileSectionId::Vertices)?;

    reader.seek(SeekFrom::Start(vis_section.offset))?;

    // visibility data
    let mut vis_data = Vec::with_capacity(vis_section.size);
    (&mut reader)
        .take(vis_section.size as u64)
        .read_to_end(&mut vis_data)?;
    table.check_end_position(&mut reader, BspFileSectionId::Visibility)?;

    // render nodes
    reader.seek(SeekFrom::Start(render_node_section.offset))?;
    debug!("Render node count = {}", render_node_count);
    let mut render_nodes = Vec::with_capacity(render_node_count);
    for _ in 0..render_node_count {
        render_nodes.push(load_render_node(&mut reader)?);
    }
    table.check_end_position(&mut reader, BspFileSectionId::RenderNodes)?;

    // texinfo
    reader.seek(SeekFrom::Start(texinfo_section.offset))?;
    let mut texinfo = Vec::with_capacity(texinfo_count);
    for _ in 0..texinfo_count {
        let mut txi = load_texinfo(&mut reader, tex_count)?;
        // !!! IMPORTANT !!!
        // remap texture ids from the on-disk ids to our ids
        txi.tex_id = texture_ids[txi.tex_id];
        texinfo.push(txi);
    }
    table.check_end_position(&mut reader, BspFileSectionId::TextureInfo)?;

    reader.seek(SeekFrom::Start(face_section.offset))?;
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
            side,
            edge_id: edge_id as usize,
            edge_count: edge_count as usize,
            texinfo_id: texinfo_id as usize,
            light_styles,
            lightmap_id,
            texture_mins: [0, 0],
            extents: [0, 0],
        });
    }
    table.check_end_position(&mut reader, BspFileSectionId::Faces)?;

    reader.seek(SeekFrom::Start(lightmap_section.offset))?;
    let mut lightmaps = Vec::with_capacity(lightmap_section.size);
    (&mut reader)
        .take(lightmap_section.size as u64)
        .read_to_end(&mut lightmaps)?;
    table.check_end_position(&mut reader, BspFileSectionId::Lightmaps)?;

    reader.seek(SeekFrom::Start(collision_node_section.offset))?;

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
            collision_node_section.offset + collision_node_section.size as u64,
        ))?
    {
        bail!("BSP read data misaligned");
    }

    reader.seek(SeekFrom::Start(leaf_section.offset))?;

    let mut leaves = Vec::with_capacity(leaf_count);
    // leaves.push(BspLeaf {
    // contents: BspLeafContents::Solid,
    // vis_offset: None,
    // min: [-32768, -32768, -32768],
    // max: [32767, 32767, 32767],
    // facelist_id: 0,
    // facelist_count: 0,
    // sounds: [0u8; NUM_AMBIENTS],
    // });

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

        let min = read_i16_3(&mut reader)?;
        let max = read_i16_3(&mut reader)?;

        let facelist_id = reader.read_u16::<LittleEndian>()? as usize;
        let facelist_count = reader.read_u16::<LittleEndian>()? as usize;
        let mut sounds = [0u8; NUM_AMBIENTS];
        reader.read_exact(&mut sounds)?;
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
    table.check_end_position(&mut reader, BspFileSectionId::Leaves)?;

    reader.seek(SeekFrom::Start(facelist_section.offset))?;
    let mut facelist = Vec::with_capacity(facelist_count);
    for _ in 0..facelist_count {
        facelist.push(reader.read_u16::<LittleEndian>()? as usize);
    }
    if reader.seek(SeekFrom::Current(0))?
        != reader.seek(SeekFrom::Start(
            facelist_section.offset + facelist_section.size as u64,
        ))?
    {
        bail!("BSP read data misaligned");
    }

    reader.seek(SeekFrom::Start(edge_section.offset))?;
    let mut edges = Vec::with_capacity(edge_count);
    for _ in 0..edge_count {
        edges.push(BspEdge {
            vertex_ids: [
                reader.read_u16::<LittleEndian>()?,
                reader.read_u16::<LittleEndian>()?,
            ],
        });
    }
    table.check_end_position(&mut reader, BspFileSectionId::Edges)?;

    reader.seek(SeekFrom::Start(edgelist_section.offset))?;
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
            edgelist_section.offset + edgelist_section.size as u64,
        ))?
    {
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

        let b_mins = [(s_min / 16.0).floor(), (t_min / 16.0).floor()];
        let b_maxs = [(s_max / 16.0).ceil(), (t_max / 16.0).ceil()];

        for i in 0..2 {
            face.texture_mins[i] = b_mins[i] as i16 * 16;
            face.extents[i] = (b_maxs[i] - b_mins[i]) as i16 * 16;

            if !texinfo.special && face.extents[i] > 2000 {
                bail!(
                    "Bad face extents: face {}, texture {}: {:?}",
                    face_id,
                    textures[texinfo.tex_id].name,
                    face.extents
                );
            }
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

    reader.seek(SeekFrom::Start(model_section.offset))?;

    let mut total_leaf_count = 0;
    let mut brush_models = Vec::with_capacity(model_count);
    for i in 0..model_count {
        // pad the bounding box by one unit in all directions
        let min = Vector3::from(read_f32_3(&mut reader)?) - Vector3::new(1.0, 1.0, 1.0);
        let max = Vector3::from(read_f32_3(&mut reader)?) + Vector3::new(1.0, 1.0, 1.0);
        let origin = read_f32_3(&mut reader)?.into();

        debug!("model[{}].min = {:?}", i, min);
        debug!("model[{}].max = {:?}", i, max);
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

    table.check_end_position(&mut reader, BspFileSectionId::Models)?;

    let models = brush_models
        .into_iter()
        .enumerate()
        .map(|(i, bmodel)| Model::from_brush_model(format!("*{}", i), bmodel))
        .collect();

    Ok((models, ent_string))
}

fn read_i16_3<R>(reader: &mut R) -> Result<[i16; 3], std::io::Error>
where
    R: ReadBytesExt,
{
    let mut ar = [0i16; 3];
    reader.read_i16_into::<LittleEndian>(&mut ar)?;
    Ok(ar)
}
