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

mod load;

use std::error::Error;
use std::fmt;
use std::ops::Deref;
use std::rc::Rc;

use chrono::Duration;
use cgmath::InnerSpace;
use cgmath::Vector3;

pub use self::load::load;

// this is 4 in the original source, but the 4th hull is never used.
const MAX_HULLS: usize = 3;

pub const MAX_LIGHTSTYLES: usize = 4;
pub const MAX_SOUNDS: usize = 4;
const MIPLEVELS: usize = 4;

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

#[derive(Copy, Clone, Debug, FromPrimitive)]
pub enum BspPlaneAxis {
    X = 0,
    Y = 1,
    Z = 2,
}

#[derive(Debug)]
pub struct BspPlane {
    /// surface normal
    normal: Vector3<f32>,

    /// distance from the map origin
    dist: f32,

    axis: Option<BspPlaneAxis>,
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
#[derive(Copy, Clone, Debug, FromPrimitive)]
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
pub struct BspCollisionHull {
    planes: Rc<Box<[BspPlane]>>,
    collision_nodes: Rc<Box<[BspCollisionNode]>>,
    collision_node_id: usize,
    collision_node_count: usize,
    mins: Vector3<f32>,
    maxs: Vector3<f32>,
}

impl BspCollisionHull {
    // TODO: see if we can't make this a little less baffling
    /// Constructs a collision hull with the given minimum and maximum bounds.
    ///
    /// This generates six planes which intersect to form a rectangular prism. The interior of the
    /// prism is `BspLeafContents::Solid`; the exterior is `BspLeafContents::Empty`.
    pub fn for_bounds(
        mins: Vector3<f32>,
        maxs: Vector3<f32>,
    ) -> Result<BspCollisionHull, BspError> {
        if mins.x >= maxs.x || mins.y >= maxs.y || mins.z >= maxs.z {
            return Err(BspError::with_msg("min bound exceeds max bound"));
        }

        let mut collision_nodes = Vec::new();
        let mut planes = Vec::new();

        // front plane (positive x)
        planes.push(BspPlane {
            normal: Vector3::unit_x(),
            dist: maxs.x,
            axis: Some(BspPlaneAxis::X),
        });
        collision_nodes.push(BspCollisionNode {
            plane_id: 0,
            front: BspCollisionNodeChild::Contents(BspLeafContents::Empty),
            back: BspCollisionNodeChild::Node(1),
        });

        // back plane (negative x)
        planes.push(BspPlane {
            normal: Vector3::unit_x(),
            dist: mins.x,
            axis: Some(BspPlaneAxis::X),
        });
        collision_nodes.push(BspCollisionNode {
            plane_id: 1,
            front: BspCollisionNodeChild::Node(2),
            back: BspCollisionNodeChild::Contents(BspLeafContents::Empty),
        });

        // left plane (positive y)
        planes.push(BspPlane {
            normal: Vector3::unit_y(),
            dist: maxs.y,
            axis: Some(BspPlaneAxis::Y),
        });
        collision_nodes.push(BspCollisionNode {
            plane_id: 2,
            front: BspCollisionNodeChild::Contents(BspLeafContents::Empty),
            back: BspCollisionNodeChild::Node(3),
        });

        // right plane (negative y)
        planes.push(BspPlane {
            normal: Vector3::unit_y(),
            dist: mins.x,
            axis: Some(BspPlaneAxis::X),
        });
        collision_nodes.push(BspCollisionNode {
            plane_id: 3,
            front: BspCollisionNodeChild::Node(4),
            back: BspCollisionNodeChild::Contents(BspLeafContents::Empty),
        });

        // top plane (positive z)
        planes.push(BspPlane {
            normal: Vector3::unit_z(),
            dist: maxs.z,
            axis: Some(BspPlaneAxis::Z),
        });
        collision_nodes.push(BspCollisionNode {
            plane_id: 4,
            front: BspCollisionNodeChild::Contents(BspLeafContents::Empty),
            back: BspCollisionNodeChild::Node(5),
        });

        // bottom plane (negative z)
        planes.push(BspPlane {
            normal: Vector3::unit_z(),
            dist: mins.z,
            axis: Some(BspPlaneAxis::Z),
        });
        collision_nodes.push(BspCollisionNode {
            plane_id: 5,
            front: BspCollisionNodeChild::Contents(BspLeafContents::Solid),
            back: BspCollisionNodeChild::Contents(BspLeafContents::Empty),
        });

        Ok(BspCollisionHull {
            planes: Rc::new(planes.into_boxed_slice()),
            collision_nodes: Rc::new(collision_nodes.into_boxed_slice()),
            collision_node_id: 0,
            collision_node_count: 6,
            mins,
            maxs,
        })
    }

    pub fn min(&self) -> Vector3<f32> {
        self.mins
    }

    pub fn max(&self) -> Vector3<f32> {
        self.maxs
    }

    /// Returns the leaf contents at the point in the given hull.
    pub fn contents_at_point(
        &self,
        node_id: usize,
        point: Vector3<f32>,
    ) -> Result<BspLeafContents, BspError> {

        let mut current_node = node_id;
        loop {
            if current_node < self.collision_node_id ||
                current_node >= self.collision_node_id + self.collision_node_count
            {
                return Err(BspError::with_msg(format!(
                    "Collision node ID out of range: was {}, must be [{}, {})",
                    current_node,
                    self.collision_node_id,
                    self.collision_node_id + self.collision_node_count
                )));
            }


            let d;
            let plane = &self.planes[self.collision_nodes[current_node].plane_id];
            match plane.axis {
                // plane is aligned along one of the major axes
                Some(a) => {
                    d = point[a as usize] - plane.dist;
                }

                // plane is not aligned, need to calculate dot product
                None => {
                    d = plane.normal.dot(point) - plane.dist;
                }
            }

            if d < 0.0 {
                current_node = match self.collision_nodes[current_node].back {
                    BspCollisionNodeChild::Node(n) => n,
                    BspCollisionNodeChild::Contents(c) => return Ok(c),
                };
            } else {
                current_node = match self.collision_nodes[current_node].front {
                    BspCollisionNodeChild::Node(n) => n,
                    BspCollisionNodeChild::Contents(c) => return Ok(c),
                };
            }
        }
    }
}

#[derive(Debug)]
struct BspLeaf {
    contents: BspLeafContents,
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
    planes: Rc<Box<[BspPlane]>>,
    textures: Box<[BspTexture]>,
    vertices: Box<[Vector3<f32>]>,
    visibility: Box<[u8]>,
    render_nodes: Box<[BspRenderNode]>,
    texinfo: Box<[BspTexInfo]>,
    faces: Box<[BspFace]>,
    lightmaps: Box<[u8]>,
    leaves: Box<[BspLeaf]>,
    facelist: Box<[usize]>,
    edges: Box<[BspEdge]>,
    edgelist: Box<[BspEdgeIndex]>,
    hulls: [BspCollisionHull; MAX_HULLS],
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
                None => panic!("Option::None value in animation sequence"),
            }

            // debug!("Frame: start {} end {} current {}", start_ms, end_ms, frame_time_ms);

            if frame_time_ms > start_ms && frame_time_ms < end_ms {
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
    collision_node_ids: [usize; MAX_HULLS],
    collision_node_counts: [usize; MAX_HULLS],
    leaf_count: usize,
    face_id: usize,
    face_count: usize,
}

impl BspModel {
    pub fn bsp_data(&self) -> Rc<BspData> {
        self.bsp_data.clone()
    }

    /// Returns the minimum extent of this BSP model.
    pub fn min(&self) -> Vector3<f32> {
        self.min
    }

    /// Returns the maximum extent of this BSP model.
    pub fn max(&self) -> Vector3<f32> {
        self.max
    }

    /// Returns the size of this BSP model.
    pub fn size(&self) -> Vector3<f32> {
        self.max - self.min
    }

    /// Returns the origin of this BSP model.
    pub fn origin(&self) -> Vector3<f32> {
        self.origin
    }

    pub fn hull(&self, index: usize) -> Result<BspCollisionHull, BspError> {
        if index > MAX_HULLS {
            return Err(BspError::with_msg(
                format!("Invalid hull index ({})", index),
            ));
        }

        let main_hull = &self.bsp_data.hulls[index];
        Ok(BspCollisionHull {
            planes: main_hull.planes.clone(),
            collision_nodes: main_hull.collision_nodes.clone(),
            collision_node_id: self.collision_node_ids[index],
            collision_node_count: self.collision_node_counts[index],
            mins: self.min,
            maxs: self.max,
        })
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
