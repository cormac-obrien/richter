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

// TODO:
// - Replace index fields with direct references where possible

//! Quake BSP file and data structure handling.
//!
//! # Data Structure
//!
//! The binary space partitioning tree, or BSP, is the central data structure used by the Quake
//! engine for collision detection and rendering level geometry. At its core, the BSP tree is a
//! binary search tree with each node representing a subspace of the map. The tree is navigated
//! using the planes stored in each node; each child represents one side of the plane.
//!
//! # File Format
//!
//! The BSP file header consists only of the file format version number, stored as an `i32`.
//!
//! This is followed by a series of "lumps" (as they are called in the Quake source code),
//! which act as a directory into the BSP file data. There are 15 of these lumps, each
//! consisting of a 32-bit offset (into the file data) and a 32-bit size (in bytes).
//!
//! ## Entities
//! Lump 0 points to the level entity data, which is stored in a JSON-like dictionary
//! format. Entities are anonymous; they do not have names, only attributes. They are stored
//! as follows:
//!
//! ```text
//! {
//! "attribute0" "value0"
//! "attribute1" "value1"
//! "attribute2" "value2"
//! }
//! {
//! "attribute0" "value0"
//! "attribute1" "value1"
//! "attribute2" "value2"
//! }
//! ```
//!
//! The newline character is `0x0A` (line feed). The entity data is stored as a null-terminated
//! string (it ends when byte `0x00` is reached).
//!
//! ## Planes
//!
//! Lump 1 points to the planes used to partition the map, stored in point-normal form as 4 IEEE 754
//! single-precision floats. The first 3 floats form the normal vector for the plane, and the last
//! float specifies the distance from the map origin along the line defined by the normal vector.
//!
//! ## Textures
//!
//! The textures are preceded by a 32-bit integer count and a list of 32-bit integer offsets. The
//! offsets are given in bytes from the beginning of the texture section (the offset given by the
//! texture lump at the start of the file).
//!
//! The textures themselves consist of a 16-byte name field, a 32-bit integer width, a 32-bit
//! integer height, and 4 32-bit mipmap offsets. These offsets are given in bytes from the beginning
//! of the texture. Each mipmap has its dimensions halved (i.e. its area quartered) from the
//! previous mipmap: the first is full size, the second 1/4, the third 1/16, and the last 1/64. Each
//! byte represents one pixel and contains an index into `gfx/palette.lmp`.
//!
//! ### Texture sequencing
//!
//! Animated textures are stored as individual frames with no guarantee of being in the correct
//! order. This means that animated textures must be sequenced when the map is loaded. Frames of
//! animated textures have names beginning with `U+002B PLUS SIGN` (`+`).
//!
//! Each texture can have two animations of up to MAX_TEXTURE_FRAMES frames each. The character
//! following the plus sign determines whether the frame belongs to the first or second animation.
//!
//! If it is between `U+0030 DIGIT ZERO` (`0`) and `U+0039 DIGIT NINE` (`9`), then the character
//! represents that texture's frame index in the first animation sequence.
//!
//! If it is between `U+0041 LATIN CAPITAL LETTER A` (`A`) and `U+004A LATIN CAPITAL LETTER J`, or
//! between `U+0061 LATIN SMALL LETTER A` (`a`) and `U+006A LATIN SMALL LETTER J`, then the
//! character represents that texture's frame index in the second animation sequence as that
//! letter's position in the English alphabet (that is, `A`/`a` correspond to 0 and `J`/`j`
//! correspond to 9).
//!
//! ## Vertex positions
//!
//! The vertex positions are stored as 3-component vectors of `float`. The Quake coordinate system
//! defines X as the longitudinal axis, Y as the lateral axis, and Z as the vertical axis.
//!
//! # Visibility lists
//!
//! The visibility lists are simply stored as a series of run-length encoded bit strings. The total
//! size of the visibility data is given by the lump size.
//!
//! ## Nodes
//!
//! Nodes are stored with a 32-bit integer plane ID denoting which plane splits the node. This is
//! followed by two 16-bit integers which point to the children in front and back of the plane. If
//! the high bit is set, the ID points to a leaf node; if not, it points to another internal node.
//!
//! After the node IDs are a 16-bit integer face ID, which denotes the index of the first face in
//! the face list that belongs to this node, and a 16-bit integer face count, which denotes the
//! number of faces to draw starting with the face ID.
//!
//! ## Edges
//!
//! The edges are stored as a pair of 16-bit integer vertex IDs.

use std::error::Error;
use std::fmt;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Cursor;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::ops::Deref;
use std::rc::Rc;

use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use chrono::Duration;
use cgmath::InnerSpace;
use cgmath::Vector3;
use num::FromPrimitive;

const VERSION: i32 = 29;

const MAX_HULLS: usize = 4;
pub const MAX_MODELS: usize = 256;
const MAX_LEAVES: usize = 32767;

// this is only used by QuakeEd
const _MAX_BRUSHES: usize = 4096;
const MAX_ENTITIES: usize = 1024;
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

const MIPLEVELS: usize = 4;
const NUM_AMBIENTS: usize = 4;
pub const MAX_LIGHTSTYLES: usize = 4;
pub const MAX_SOUNDS: usize = 4;
const MAX_TEXTURE_FRAMES: usize = 10;
const TEXTURE_FRAME_LEN_MS: i64 = 200;

const ASCII_0: usize = '0' as usize;
const ASCII_9: usize = '9' as usize;
const ASCII_CAPITAL_A: usize = 'A' as usize;
const ASCII_CAPITAL_J: usize = 'J' as usize;
const ASCII_SMALL_A: usize = 'a' as usize;
const ASCII_SMALL_J: usize = 'j' as usize;

#[derive(Debug)]
pub enum BspError {
    Io(::std::io::Error),
    Other(String),
}

impl BspError {
    fn with_msg<S>(msg: S) -> Self
    where
        S: AsRef<str>,
    {
        BspError::Other(msg.as_ref().to_owned())
    }
}

impl fmt::Display for BspError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            BspError::Io(ref err) => err.fmt(f),
            BspError::Other(ref msg) => write!(f, "{}", msg),
        }
    }
}

impl Error for BspError {
    fn description(&self) -> &str {
        match *self {
            BspError::Io(ref err) => err.description(),
            BspError::Other(ref msg) => &msg,
        }
    }
}

impl From<::std::io::Error> for BspError {
    fn from(error: ::std::io::Error) -> Self {
        BspError::Io(error)
    }
}

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

#[derive(Debug, FromPrimitive)]
enum BspPlaneKind {
    X = 0,
    Y = 1,
    Z = 2,
    AnyX = 3,
    AnyY = 4,
    AnyZ = 5,
}

#[derive(Debug)]
struct BspPlane {
    /// surface normal
    normal: Vector3<f32>,

    /// distance from the map origin
    dist: f32,

    kind: BspPlaneKind,
}

#[derive(Copy, Clone, Debug)]
pub enum BspTextureMipmap {
    Full = 0,
    Half = 1,
    Quarter = 2,
    Eighth = 3,
}

#[derive(Debug)]
pub struct BspTextureAnimation {
    sequence_duration: Duration,
    time_start: Duration,
    time_end: Duration,
    next: usize,
}

#[derive(Debug)]
pub struct BspTexture {
    name: String,
    width: u32,
    height: u32,
    mipmaps: [Vec<u8>; MIPLEVELS],
    animation: Option<BspTextureAnimation>,
}

impl BspTexture {
    /// Returns the name of the texture.
    pub fn name(&self) -> &str {
        self.name.as_ref()
    }

    /// Returns a tuple containing the width and height of the texture.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Returns the texture's mipmap of the specified level.
    pub fn mipmap(&self, mipmap: BspTextureMipmap) -> &[u8] {
        &self.mipmaps[mipmap as usize]
    }
}

#[derive(Debug)]
enum BspRenderNodeChild {
    Node(usize),
    Leaf(usize),
}

#[derive(Debug)]
struct BspRenderNode {
    plane_id: usize,
    front: BspRenderNodeChild,
    back: BspRenderNodeChild,
    min: [i16; 3],
    max: [i16; 3],
    face_id: usize,
    face_count: usize,
}

#[derive(Debug)]
struct BspTexInfo {
    s_vector: Vector3<f32>,
    s_offset: f32,
    t_vector: Vector3<f32>,
    t_offset: f32,
    tex_id: usize,
    animated: bool,
}

#[derive(Copy, Clone, Debug)]
enum BspFaceSide {
    Front,
    Back,
}

#[derive(Debug)]
pub struct BspFace {
    plane_id: usize,
    side: BspFaceSide,
    edge_id: usize,
    edge_count: usize,
    texinfo_id: usize,
    light_styles: [u8; MAX_LIGHTSTYLES],
    lightmap_id: Option<usize>,
}

/// The contents of a leaf in the BSP tree, specifying how it should look and behave.
#[derive(Debug, FromPrimitive)]
pub enum BspLeafContents {
    /// The leaf has nothing in it. Vision is unobstructed and movement is unimpeded.
    Empty = 1,

    /// The leaf is solid. Physics objects will collide with its surface and may not move inside it.
    Solid = 2,

    /// The leaf is full of water. Vision is warped to simulate refraction and movement is done by
    /// swimming instead of walking.
    Water = 3,

    /// The leaf is full of acidic slime. Vision is tinted green, movement is done by swimming and
    /// entities take periodic minor damage.
    Slime = 4,

    /// The leaf is full of lava. Vision is tinted red, movement is done by swimming and entities
    /// take periodic severe damage.
    Lava = 5,

    // This doesn't appear to ever be used
    // Sky = 6,

    // This is removed during map compilation
    // Origin = 7,

    // This is converted to `BspLeafContents::Solid`
    // Collide = 8,
    /// Same as `BspLeafContents::Water`, but the player is constantly pushed in the positive
    /// x-direction (east).
    Current0 = 9,

    /// Same as `BspLeafContents::Water`, but the player is constantly pushed in the positive
    /// y-direction (north).
    Current90 = 10,

    /// Same as `BspLeafContents::Water`, but the player is constantly pushed in the negative
    /// x-direction (west).
    Current180 = 11,

    /// Same as `BspLeafContents::Water`, but the player is constantly pushed in the negative
    /// y-direction (south).
    Current270 = 12,

    /// Same as `BspLeafContents::Water`, but the player is constantly pushed in the positive
    /// z-direction (up).
    CurrentUp = 13,

    /// Same as `BspLeafContents::Water`, but the player is constantly pushed in the negative
    /// z-direction (down).
    CurrentDown = 14,
}

#[derive(Debug)]
enum BspCollisionNodeChild {
    Node(usize),
    Contents(BspLeafContents),
}

#[derive(Debug)]
pub struct BspCollisionNode {
    plane_id: usize,
    front: BspCollisionNodeChild,
    back: BspCollisionNodeChild,
}

#[derive(Debug)]
struct BspLeaf {
    contents: i32,
    vis_offset: Option<usize>,
    min: [i16; 3],
    max: [i16; 3],
    face_id: usize,
    face_count: usize,
    sounds: [u8; MAX_SOUNDS],
}

#[derive(Debug)]
struct BspEdge {
    vertex_ids: [u16; 2],
}

#[derive(Copy, Clone, Debug)]
enum BspEdgeDirection {
    Forward = 0,
    Backward = 1,
}

#[derive(Debug)]
struct BspEdgeIndex {
    direction: BspEdgeDirection,
    index: usize,
}

#[derive(Debug)]
pub struct BspData {
    planes: Box<[BspPlane]>,
    textures: Box<[BspTexture]>,
    vertices: Box<[Vector3<f32>]>,
    visibility: Box<[u8]>,
    render_nodes: Box<[BspRenderNode]>,
    texinfo: Box<[BspTexInfo]>,
    faces: Box<[BspFace]>,
    lightmaps: Box<[u8]>,
    collision_nodes: Box<[BspCollisionNode]>,
    leaves: Box<[BspLeaf]>,
    facelist: Box<[usize]>,
    edges: Box<[BspEdge]>,
    edgelist: Box<[BspEdgeIndex]>,
}

impl BspData {
    pub fn textures(&self) -> &[BspTexture] {
        &self.textures
    }

    /// Find the index of the appropriate frame of the texture with index `first`.
    ///
    /// If the texture is not animated, immediately returns `first`.
    pub fn texture_frame_for_time(&self, first: usize, time: Duration) -> usize {
        let frame_time_ms = match self.textures[first].animation {
            Some(ref a) => {
                let sequence_ms = a.sequence_duration.num_milliseconds();
                let time_ms = time.num_milliseconds();
                time_ms % sequence_ms
            }
            None => return first,
        };

        let mut frame_id = first;
        loop {
            // TODO: this destructuring is a bit unwieldy, maybe see if we can change texture
            // sequences to remove the Option types
            let start_ms;
            let end_ms;
            let next;
            match self.textures[frame_id].animation {
                Some(ref a) => {
                    start_ms = a.time_start.num_milliseconds();
                    end_ms = a.time_end.num_milliseconds();
                    next = a.next;
                }
                None => panic!("Option::None value in animation sequence")
            }

            // debug!("Frame: start {} end {} current {}", start_ms, end_ms, frame_time_ms);

            if frame_time_ms > start_ms && frame_time_ms < end_ms {
                debug!("Using texture {}", self.textures[frame_id].name);
                return frame_id;
            }

            frame_id = next;

            // if we get in an infinite cycle, just return the first texture.
            if frame_id == first {
                return first;
            }
        }
    }
}

#[derive(Debug)]
pub struct BspModel {
    bsp_data: Rc<BspData>,
    min: Vector3<f32>,
    max: Vector3<f32>,
    origin: Vector3<f32>,
    roots: [i32; MAX_HULLS],
    leaf_count: usize,
    face_id: usize,
    face_count: usize,
}

impl BspModel {
    pub fn bsp_data(&self) -> Rc<BspData> {
        self.bsp_data.clone()
    }
}

impl BspModel {
    pub fn size(&self) -> Vector3<f32> {
        self.max - self.min
    }
}

#[derive(Debug)]
pub struct WorldModel(BspModel);

impl Deref for WorldModel {
    type Target = BspModel;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl WorldModel {
    /// Locates the leaf containing the given position vector and returns its index.
    pub fn find_leaf<V>(&self, pos: V) -> usize
    where
        V: Into<Vector3<f32>>,
    {
        let pos_vec = pos.into();

        let mut node = &self.bsp_data.render_nodes[0];
        loop {
            let plane = &self.bsp_data.planes[node.plane_id];

            let child;
            if pos_vec.dot(Vector3::from(plane.normal)) - plane.dist < 0.0 {
                child = &node.front;
            } else {
                child = &node.back;
            }

            match child {
                &BspRenderNodeChild::Node(i) => node = &self.bsp_data.render_nodes[i],
                &BspRenderNodeChild::Leaf(i) => return i,
            }
        }
    }
}

pub fn load(data: &[u8]) -> Result<(WorldModel, Box<[BspModel]>, String), BspError> {
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
            "Lump {:>2}: Offset = 0x{:>08x} | Size = 0x{:>08x}",
            l,
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
            kind: BspPlaneKind::from_i32(reader.read_i32::<LittleEndian>()?).unwrap(),
        });
    }
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
    let mut render_nodes = Vec::with_capacity(render_node_count);
    for _ in 0..render_node_count {
        let plane_id = reader.read_i32::<LittleEndian>()?;
        if plane_id < 0 {
            return Err(BspError::with_msg("Invalid plane id"));
        }

        let front = match reader.read_i16::<LittleEndian>()? {
            f if (f >> 15) & 1 == 1 => BspRenderNodeChild::Leaf(f as usize),
            f => BspRenderNodeChild::Node(f as usize),
        };

        let back = match reader.read_i16::<LittleEndian>()? {
            b if (b >> 15) & 1 == 1 => BspRenderNodeChild::Leaf(b as usize),
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
            front: front,
            back: back,
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
            "Clipnode count ({}) exceeds MAX_COLLISION_NODES ({})",
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
            front,
            back,
        });
    }

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
        let contents = reader.read_i32::<LittleEndian>()?;
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

    let bsp_data = Rc::new(BspData {
        planes: planes.into_boxed_slice(),
        textures: textures.into_boxed_slice(),
        vertices: vertices.into_boxed_slice(),
        visibility: vis_data.into_boxed_slice(),
        render_nodes: render_nodes.into_boxed_slice(),
        texinfo: texinfo.into_boxed_slice(),
        faces: faces.into_boxed_slice(),
        lightmaps: lightmaps.into_boxed_slice(),
        collision_nodes: collision_nodes.into_boxed_slice(),
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

    let mut models = Vec::with_capacity(model_count);
    for i in 0..model_count {
        let min = Vector3::new(
            reader.read_f32::<LittleEndian>()?,
            reader.read_f32::<LittleEndian>()?,
            reader.read_f32::<LittleEndian>()?,
        );

        debug!("model[{}].min = {:?}", i, min);

        let max = Vector3::new(
            reader.read_f32::<LittleEndian>()?,
            reader.read_f32::<LittleEndian>()?,
            reader.read_f32::<LittleEndian>()?,
        );

        debug!("model[{}].max = {:?}", i, max);

        let origin = Vector3::new(
            reader.read_f32::<LittleEndian>()?,
            reader.read_f32::<LittleEndian>()?,
            reader.read_f32::<LittleEndian>()?,
        );

        debug!("model[{}].origin = {:?}", i, max);

        let mut roots = [0; MAX_HULLS];
        for i in 0..roots.len() {
            roots[i] = reader.read_i32::<LittleEndian>()?;
        }

        debug!("model[{}].headnodes = {:?}", i, roots);

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

        models.push(BspModel {
            bsp_data: bsp_data.clone(),
            min: min,
            max: max,
            origin: origin,
            roots: roots,
            leaf_count: leaf_count,
            face_id: face_id,
            face_count: face_count,
        });
    }

    let mut models_iter = models.into_iter();
    let world_model = WorldModel(models_iter.next().unwrap());
    let sub_models = models_iter.collect::<Vec<_>>().into_boxed_slice();

    if reader.seek(SeekFrom::Current(0))? !=
        reader.seek(SeekFrom::Start(
            model_lump.offset + model_lump.size as u64,
        ))?
    {
        return Err(BspError::with_msg("BSP read data misaligned"));
    }

    Ok((world_model, sub_models, ent_string))
}

impl BspData {
    /// Decompresses the PVS for the leaf with the given ID
    pub fn decompress_visibility(&self, leaf_id: usize) -> Option<Vec<u8>> {
        // Calculate length of vis data in bytes, rounding up
        let decompressed_len = (self.leaves.len() + 7) / 8;

        match self.leaves[leaf_id].vis_offset {
            Some(o) => {
                let mut decompressed = Vec::new();

                let mut i = 0;
                while decompressed.len() < decompressed_len {
                    match self.visibility[o + i] {
                        0 => {
                            let count = self.visibility[o + i + 1];
                            for _ in 0..count {
                                decompressed.push(0);
                            }
                        }
                        x => decompressed.push(x),
                    }
                }

                assert_eq!(decompressed.len(), decompressed_len);

                Some(decompressed)
            }
            None => None,
        }
    }

    /// Maps a function over the BSP textures and returns a vector of the results.
    ///
    /// This is meant to be used to provide a straightforward method of generating texture objects
    /// for graphics APIs like OpenGL.
    pub fn gen_textures<F, T>(&self, mut func: F) -> Vec<T>
    where
        F: FnMut(&BspTexture) -> T,
    {
        self.textures.iter().map(|tex| func(tex)).collect()
    }

    /// Generates render data in interleaved format.
    pub fn gen_render_data_interleaved<F, V>(&self) -> (Vec<F>, Vec<V>)
    where
        F: From<(usize, usize, usize)>,
        V: From<[f32; 5]>,
    {
        let mut face_data = Vec::new();
        let mut vertex_data = Vec::new();

        for face in self.faces.iter() {
            let face_vertex_id = vertex_data.len();

            let texinfo = &self.texinfo[face.texinfo_id];
            let tex = &self.textures[texinfo.tex_id];

            // Convert from triangle-fan to triangle-list format
            let face_edge_ids = &self.edgelist[face.edge_id..face.edge_id + face.edge_count];

            // Store the data for the base vertex of the fan
            let base_edge_id = &face_edge_ids[0];
            let base_vertex_id = self.edges[base_edge_id.index].vertex_ids[base_edge_id.direction as
                                                                               usize];
            let base_position = self.vertices[base_vertex_id as usize];
            let base_pos_vec = Vector3::from(base_position);
            let base_s = (base_pos_vec.dot(Vector3::from(texinfo.s_vector)) + texinfo.s_offset) /
                tex.width as f32;
            let base_t = (base_pos_vec.dot(Vector3::from(texinfo.t_vector)) + texinfo.t_offset) /
                tex.height as f32;

            // Duplicate every subsequent pair of vertices in the fan
            for i in 1..face_edge_ids.len() - 1 {
                // First push the base vertex
                vertex_data.push(
                    [
                        base_position[0],
                        base_position[1],
                        base_position[2],
                        base_s,
                        base_t,
                    ],
                );

                // And then the vertices comprising the next section of the fan
                for v in 0..2 {
                    let edge_id = &face_edge_ids[i + v];
                    let vertex_id = self.edges[edge_id.index].vertex_ids[edge_id.direction as
                                                                             usize];
                    let position = self.vertices[vertex_id as usize];
                    let pos_vec = Vector3::from(self.vertices[vertex_id as usize]);
                    let s = (pos_vec.dot(Vector3::from(texinfo.s_vector)) + texinfo.s_offset) /
                        tex.width as f32;
                    let t = (pos_vec.dot(Vector3::from(texinfo.t_vector)) + texinfo.t_offset) /
                        tex.height as f32;

                    vertex_data.push([position[0], position[1], position[2], s, t]);
                }
            }

            let face_vertex_count = vertex_data.len() - face_vertex_id;
            face_data.push((
                face_vertex_id,
                face_vertex_count,
                self.texinfo[face.texinfo_id].tex_id,
            ));
        }

        (
            face_data.into_iter().map(|f| F::from(f)).collect(),
            vertex_data.into_iter().map(|v| V::from(v)).collect(),
        )
    }
}
