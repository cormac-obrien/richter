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

use std::{collections::HashSet, error::Error, fmt, iter::Iterator, rc::Rc};

use crate::common::math::{Hyperplane, HyperplaneSide, LinePlaneIntersect};

// TODO: Either Trace should be moved into common or the functions requiring it should be moved into server
use crate::server::world::{Trace, TraceEnd, TraceStart};

use cgmath::Vector3;
use chrono::Duration;

pub use self::load::load;

// this is 4 in the original source, but the 4th hull is never used.
const MAX_HULLS: usize = 3;

pub const MAX_LIGHTMAPS: usize = 64;
pub const MAX_LIGHTSTYLES: usize = 4;
pub const MAX_SOUNDS: usize = 4;
pub const MIPLEVELS: usize = 4;
const DIST_EPSILON: f32 = 0.03125;

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
pub enum BspTextureMipmap {
    Full = 0,
    Half = 1,
    Quarter = 2,
    Eighth = 3,
}

#[derive(Debug)]
pub struct BspTextureAnimation {
    pub sequence_duration: Duration,
    pub time_start: Duration,
    pub time_end: Duration,
    pub next: usize,
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

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Returns a tuple containing the width and height of the texture.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Returns the texture's mipmap of the specified level.
    pub fn mipmap(&self, mipmap: BspTextureMipmap) -> &[u8] {
        &self.mipmaps[mipmap as usize]
    }

    /// Returns this texture's animation data, if any.
    pub fn animation(&self) -> Option<&BspTextureAnimation> {
        self.animation.as_ref()
    }
}

#[derive(Debug)]
pub enum BspRenderNodeChild {
    Node(usize),
    Leaf(usize),
}

#[derive(Debug)]
pub struct BspRenderNode {
    pub plane_id: usize,
    pub children: [BspRenderNodeChild; 2],
    pub min: [i16; 3],
    pub max: [i16; 3],
    pub face_id: usize,
    pub face_count: usize,
}

#[derive(Debug)]
pub struct BspTexInfo {
    pub s_vector: Vector3<f32>,
    pub s_offset: f32,
    pub t_vector: Vector3<f32>,
    pub t_offset: f32,
    pub tex_id: usize,
    pub special: bool,
}

#[derive(Copy, Clone, Debug)]
pub enum BspFaceSide {
    Front,
    Back,
}

#[derive(Debug)]
pub struct BspFace {
    pub plane_id: usize,
    pub side: BspFaceSide,
    pub edge_id: usize,
    pub edge_count: usize,
    pub texinfo_id: usize,
    pub light_styles: [u8; MAX_LIGHTSTYLES],
    pub lightmap_id: Option<usize>,

    pub texture_mins: [i16; 2],
    pub extents: [i16; 2],
}

/// The contents of a leaf in the BSP tree, specifying how it should look and behave.
#[derive(Copy, Clone, Debug, Eq, FromPrimitive, PartialEq)]
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
pub enum BspCollisionNodeChild {
    Node(usize),
    Contents(BspLeafContents),
}

#[derive(Debug)]
pub struct BspCollisionNode {
    plane_id: usize,
    children: [BspCollisionNodeChild; 2],
}

#[derive(Debug)]
pub struct BspCollisionHull {
    planes: Rc<Box<[Hyperplane]>>,
    nodes: Rc<Box<[BspCollisionNode]>>,
    node_id: usize,
    node_count: usize,
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
        debug!(
            "Generating collision hull for min = {:?} max = {:?}",
            mins, maxs
        );

        if mins.x >= maxs.x || mins.y >= maxs.y || mins.z >= maxs.z {
            return Err(BspError::with_msg("min bound exceeds max bound"));
        }

        let mut nodes = Vec::new();
        let mut planes = Vec::new();

        // front plane (positive x)
        planes.push(Hyperplane::axis_x(maxs.x));
        nodes.push(BspCollisionNode {
            plane_id: 0,
            children: [
                BspCollisionNodeChild::Contents(BspLeafContents::Empty),
                BspCollisionNodeChild::Node(1),
            ],
        });

        // back plane (negative x)
        planes.push(Hyperplane::axis_x(mins.x));
        nodes.push(BspCollisionNode {
            plane_id: 1,
            children: [
                BspCollisionNodeChild::Node(2),
                BspCollisionNodeChild::Contents(BspLeafContents::Empty),
            ],
        });

        // left plane (positive y)
        planes.push(Hyperplane::axis_y(maxs.y));
        nodes.push(BspCollisionNode {
            plane_id: 2,
            children: [
                BspCollisionNodeChild::Contents(BspLeafContents::Empty),
                BspCollisionNodeChild::Node(3),
            ],
        });

        // right plane (negative y)
        planes.push(Hyperplane::axis_y(mins.y));
        nodes.push(BspCollisionNode {
            plane_id: 3,
            children: [
                BspCollisionNodeChild::Node(4),
                BspCollisionNodeChild::Contents(BspLeafContents::Empty),
            ],
        });

        // top plane (positive z)
        planes.push(Hyperplane::axis_z(maxs.z));
        nodes.push(BspCollisionNode {
            plane_id: 4,
            children: [
                BspCollisionNodeChild::Contents(BspLeafContents::Empty),
                BspCollisionNodeChild::Node(5),
            ],
        });

        // bottom plane (negative z)
        planes.push(Hyperplane::axis_z(mins.z));
        nodes.push(BspCollisionNode {
            plane_id: 5,
            children: [
                BspCollisionNodeChild::Contents(BspLeafContents::Solid),
                BspCollisionNodeChild::Contents(BspLeafContents::Empty),
            ],
        });

        Ok(BspCollisionHull {
            planes: Rc::new(planes.into_boxed_slice()),
            nodes: Rc::new(nodes.into_boxed_slice()),
            node_id: 0,
            node_count: 6,
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

    /// Returns the leaf contents at the given point in this hull.
    pub fn contents_at_point(&self, point: Vector3<f32>) -> Result<BspLeafContents, BspError> {
        self.contents_at_point_node(self.node_id, point)
    }

    fn contents_at_point_node(
        &self,
        node: usize,
        point: Vector3<f32>,
    ) -> Result<BspLeafContents, BspError> {
        let mut current_node = &self.nodes[node];

        loop {
            let ref plane = self.planes[current_node.plane_id];

            match current_node.children[plane.point_side(point) as usize] {
                BspCollisionNodeChild::Contents(c) => return Ok(c),
                BspCollisionNodeChild::Node(n) => current_node = &self.nodes[n],
            }
        }
    }

    pub fn trace(&self, start: Vector3<f32>, end: Vector3<f32>) -> Result<Trace, BspError> {
        self.recursive_trace(self.node_id, start, end)
    }

    fn recursive_trace(
        &self,
        node: usize,
        start: Vector3<f32>,
        end: Vector3<f32>,
    ) -> Result<Trace, BspError> {
        debug!("start={:?} end={:?}", start, end);
        let ref node = self.nodes[node];
        let ref plane = self.planes[node.plane_id];

        match plane.line_segment_intersection(start, end) {
            // start -> end falls entirely on one side of the plane
            LinePlaneIntersect::NoIntersection(side) => {
                debug!("No intersection");
                match node.children[side as usize] {
                    // this is an internal node, keep searching for a leaf
                    BspCollisionNodeChild::Node(n) => {
                        debug!("Descending to {:?} node with ID {}", side, n);
                        self.recursive_trace(n, start, end)
                    }

                    // start -> end falls entirely inside a leaf
                    BspCollisionNodeChild::Contents(c) => {
                        debug!("Found leaf with contents {:?}", c);
                        Ok(Trace::new(
                            TraceStart::new(start, 0.0),
                            TraceEnd::terminal(end),
                            c,
                        ))
                    }
                }
            }

            // start -> end crosses the plane at one point
            LinePlaneIntersect::PointIntersection(point_intersect) => {
                let near_side = plane.point_side(start);
                let far_side = plane.point_side(end);
                let mid = point_intersect.point();
                let ratio = point_intersect.ratio();
                debug!("Intersection at {:?} (ratio={})", mid, ratio);

                // calculate the near subtrace
                let near = match node.children[near_side as usize] {
                    BspCollisionNodeChild::Node(near_n) => {
                        debug!(
                            "Descending to near ({:?}) node with ID {}",
                            near_side, near_n
                        );
                        self.recursive_trace(near_n, start, mid)?
                    }
                    BspCollisionNodeChild::Contents(near_c) => {
                        debug!("Found near leaf with contents {:?}", near_c);
                        Trace::new(
                            TraceStart::new(start, 0.0),
                            TraceEnd::boundary(
                                mid,
                                ratio,
                                match near_side {
                                    HyperplaneSide::Positive => plane.to_owned(),
                                    HyperplaneSide::Negative => -plane.to_owned(),
                                },
                            ),
                            near_c,
                        )
                    }
                };

                // check for an early collision
                if near.is_terminal() || near.end_point() != point_intersect.point() {
                    return Ok(near);
                }

                // if we haven't collided yet, calculate the far subtrace
                let far = match node.children[far_side as usize] {
                    BspCollisionNodeChild::Node(far_n) => {
                        debug!("Descending to far ({:?}) node with ID {}", far_side, far_n);
                        self.recursive_trace(far_n, mid, end)?
                    }
                    BspCollisionNodeChild::Contents(far_c) => {
                        debug!("Found far leaf with contents {:?}", far_c);
                        Trace::new(TraceStart::new(mid, ratio), TraceEnd::terminal(end), far_c)
                    }
                };

                // check for collision and join traces accordingly
                Ok(near.join(far))
            }
        }
    }

    pub fn gen_dot_graph(&self) -> String {
        let mut dot = String::new();
        dot += "digraph hull {\n";
        dot += "    rankdir=LR\n";

        let mut rank_lists = Vec::new();
        let mut leaf_names = Vec::new();

        dot += &self.gen_dot_graph_recursive(0, &mut rank_lists, &mut leaf_names, self.node_id);

        for rank in rank_lists {
            dot += "    {rank=same;";
            for node_id in rank {
                dot += &format!("n{},", node_id);
            }
            // discard trailing comma
            dot.pop().unwrap();
            dot += "}\n"
        }

        dot += "    {rank=same;";
        for leaf in leaf_names {
            dot += &format!("{},", leaf);
        }
        // discard trailing comma
        dot.pop().unwrap();
        dot.pop().unwrap();
        dot += "}\n";

        dot += "}";

        dot
    }

    fn gen_dot_graph_recursive(
        &self,
        rank: usize,
        rank_lists: &mut Vec<HashSet<usize>>,
        leaf_names: &mut Vec<String>,
        node_id: usize,
    ) -> String {
        let mut result = String::new();

        if rank >= rank_lists.len() {
            rank_lists.push(HashSet::new());
        }

        rank_lists[rank].insert(node_id);

        for child in self.nodes[node_id].children.iter() {
            match child {
                &BspCollisionNodeChild::Node(n) => {
                    result += &format!("    n{} -> n{}\n", node_id, n);
                    result += &self.gen_dot_graph_recursive(rank + 1, rank_lists, leaf_names, n);
                }
                &BspCollisionNodeChild::Contents(_) => {
                    let leaf_count = leaf_names.len();
                    let leaf_name = format!("l{}", leaf_count);
                    result += &format!("    n{} -> {}\n", node_id, leaf_name);
                    leaf_names.push(leaf_name);
                }
            }
        }

        result
    }
}

#[derive(Debug)]
pub struct BspLeaf {
    pub contents: BspLeafContents,
    pub vis_offset: Option<usize>,
    pub min: [i16; 3],
    pub max: [i16; 3],
    pub facelist_id: usize,
    pub facelist_count: usize,
    pub sounds: [u8; MAX_SOUNDS],
}

#[derive(Debug)]
pub struct BspEdge {
    pub vertex_ids: [u16; 2],
}

#[derive(Copy, Clone, Debug)]
pub enum BspEdgeDirection {
    Forward = 0,
    Backward = 1,
}

#[derive(Debug)]
pub struct BspEdgeIndex {
    pub direction: BspEdgeDirection,
    pub index: usize,
}

#[derive(Debug)]
pub struct BspLightmap<'a> {
    width: u32,
    height: u32,
    data: &'a [u8],
}

impl<'a> BspLightmap<'a> {
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn data(&self) -> &[u8] {
        self.data
    }
}

#[derive(Debug)]
pub struct BspData {
    pub(crate) planes: Rc<Box<[Hyperplane]>>,
    pub(crate) textures: Box<[BspTexture]>,
    pub(crate) vertices: Box<[Vector3<f32>]>,
    pub(crate) visibility: Box<[u8]>,
    pub(crate) render_nodes: Box<[BspRenderNode]>,
    pub(crate) texinfo: Box<[BspTexInfo]>,
    pub(crate) faces: Box<[BspFace]>,
    pub(crate) lightmaps: Box<[u8]>,
    pub(crate) leaves: Box<[BspLeaf]>,
    pub(crate) facelist: Box<[usize]>,
    pub(crate) edges: Box<[BspEdge]>,
    pub(crate) edgelist: Box<[BspEdgeIndex]>,
    pub(crate) hulls: [BspCollisionHull; MAX_HULLS],
}

impl BspData {
    pub fn planes(&self) -> &[Hyperplane] {
        &self.planes
    }

    pub fn textures(&self) -> &[BspTexture] {
        &self.textures
    }

    pub fn vertices(&self) -> &[Vector3<f32>] {
        &self.vertices
    }

    pub fn render_nodes(&self) -> &[BspRenderNode] {
        &self.render_nodes
    }

    pub fn texinfo(&self) -> &[BspTexInfo] {
        &self.texinfo
    }

    pub fn face(&self, face_id: usize) -> &BspFace {
        &self.faces[face_id]
    }

    pub fn face_iter_vertices(&self, face_id: usize) -> impl Iterator<Item = Vector3<f32>> + '_ {
        let face = &self.faces[face_id];
        self.edgelist[face.edge_id..face.edge_id + face.edge_count]
            .iter()
            .map(move |id| {
                self.vertices[self.edges[id.index].vertex_ids[id.direction as usize] as usize]
            })
    }

    pub fn face_texinfo(&self, face_id: usize) -> &BspTexInfo {
        &self.texinfo[self.faces[face_id].texinfo_id]
    }

    pub fn face_lightmaps(&self, face_id: usize) -> Vec<BspLightmap> {
        let face = &self.faces[face_id];
        match face.lightmap_id {
            Some(lightmap_id) => {
                let lightmap_w = face.extents[0] as u32 / 16 + 1;
                let lightmap_h = face.extents[1] as u32 / 16 + 1;
                let lightmap_size = (lightmap_w * lightmap_h) as usize;

                face.light_styles
                    .iter()
                    .take_while(|style| **style != 255)
                    .enumerate()
                    .map(|(i, _)| {
                        let start = lightmap_id + lightmap_size * i as usize;
                        let end = start + lightmap_size;
                        BspLightmap {
                            width: lightmap_w,
                            height: lightmap_h,
                            data: &self.lightmaps[start..end],
                        }
                    })
                    .collect()
            }
            None => Vec::new(),
        }
    }

    pub fn faces(&self) -> &[BspFace] {
        &self.faces
    }

    pub fn lightmaps(&self) -> &[u8] {
        &self.lightmaps
    }

    pub fn leaves(&self) -> &[BspLeaf] {
        &self.leaves
    }

    pub fn facelist(&self) -> &[usize] {
        &self.facelist
    }

    pub fn edges(&self) -> &[BspEdge] {
        &self.edges
    }

    pub fn edgelist(&self) -> &[BspEdgeIndex] {
        &self.edgelist
    }

    pub fn hulls(&self) -> &[BspCollisionHull] {
        &self.hulls
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

    /// Locates the leaf containing the given position vector and returns its index.
    pub fn find_leaf<V>(&self, pos: V) -> usize
    where
        V: Into<Vector3<f32>>,
    {
        let pos_vec = pos.into();

        let mut node = &self.render_nodes[0];
        loop {
            let plane = &self.planes[node.plane_id];

            match node.children[plane.point_side(pos_vec) as usize] {
                BspRenderNodeChild::Node(node_id) => {
                    node = &self.render_nodes[node_id];
                }
                BspRenderNodeChild::Leaf(leaf_id) => return leaf_id,
            }
        }
    }

    pub fn get_pvs(&self, leaf_id: usize, leaf_count: usize) -> Vec<usize> {
        // leaf 0 is outside the map, everything is visible
        if leaf_id == 0 {
            return Vec::new();
        }

        match self.leaves[leaf_id].vis_offset {
            Some(o) => {
                let mut visleaf = 1;
                let mut visleaf_list = Vec::new();
                let mut it = (&self.visibility[o..]).iter();

                while visleaf < leaf_count {
                    let byte = it.next().unwrap();
                    match *byte {
                        // a zero byte signals the start of an RLE sequence
                        0 => visleaf += 8 * *it.next().unwrap() as usize,

                        bits => {
                            for shift in 0..8 {
                                if bits & 1 << shift != 0 {
                                    visleaf_list.push(visleaf);
                                }

                                visleaf += 1;
                            }
                        }
                    }
                }

                visleaf_list
            }

            None => Vec::new(),
        }
    }

    pub fn gen_dot_graph(&self) -> String {
        let mut dot = String::new();
        dot += "digraph render {\n";
        dot += "    rankdir=LR\n";

        let mut rank_lists = Vec::new();
        let mut leaf_names = Vec::new();

        dot += &self.gen_dot_graph_recursive(0, &mut rank_lists, &mut leaf_names, 0);

        for rank in rank_lists {
            dot += "    {rank=same;";
            for node_id in rank {
                dot += &format!("n{},", node_id);
            }
            // discard trailing comma
            dot.pop().unwrap();
            dot += "}\n"
        }

        dot += "    {rank=same;";
        for leaf_id in 1..self.leaves().len() {
            dot += &format!("l{},", leaf_id);
        }
        // discard trailing comma
        dot.pop().unwrap();
        dot += "}\n";

        dot += "}";

        dot
    }

    fn gen_dot_graph_recursive(
        &self,
        rank: usize,
        rank_lists: &mut Vec<HashSet<usize>>,
        leaf_names: &mut Vec<String>,
        node_id: usize,
    ) -> String {
        let mut result = String::new();

        if rank >= rank_lists.len() {
            rank_lists.push(HashSet::new());
        }

        rank_lists[rank].insert(node_id);

        for child in self.render_nodes[node_id].children.iter() {
            match *child {
                BspRenderNodeChild::Node(n) => {
                    result += &format!("    n{} -> n{}\n", node_id, n);
                    result += &self.gen_dot_graph_recursive(rank + 1, rank_lists, leaf_names, n);
                }
                BspRenderNodeChild::Leaf(leaf_id) => match leaf_id {
                    0 => {
                        result += &format!(
                            "    l0_{0} [shape=point label=\"\"]\n    n{0} -> l0_{0}\n",
                            node_id
                        );
                    }
                    _ => result += &format!("    n{} -> l{}\n", node_id, leaf_id),
                },
            }
        }

        result
    }
}

#[derive(Debug)]
pub struct BspModel {
    pub bsp_data: Rc<BspData>,
    pub min: Vector3<f32>,
    pub max: Vector3<f32>,
    pub origin: Vector3<f32>,
    pub collision_node_ids: [usize; MAX_HULLS],
    pub collision_node_counts: [usize; MAX_HULLS],
    pub leaf_id: usize,
    pub leaf_count: usize,
    pub face_id: usize,
    pub face_count: usize,
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

    pub fn iter_leaves(&self) -> impl Iterator<Item = &BspLeaf> {
        // add 1 to leaf_count because...??? TODO: figure out if this is documented anywhere
        self.bsp_data.leaves[self.leaf_id..self.leaf_id + self.leaf_count + 1].iter()
    }

    pub fn iter_faces(&self) -> impl Iterator<Item = &BspFace> {
        self.bsp_data.facelist[self.face_id..self.face_id + self.face_count]
            .iter()
            .map(move |face_id| &self.bsp_data.faces[*face_id])
    }

    pub fn face_list(&self) -> &[usize] {
        &self.bsp_data.facelist[self.face_id..self.face_id + self.face_count]
    }

    pub fn hull(&self, index: usize) -> Result<BspCollisionHull, BspError> {
        if index > MAX_HULLS {
            return Err(BspError::with_msg(format!(
                "Invalid hull index ({})",
                index
            )));
        }

        let main_hull = &self.bsp_data.hulls[index];
        Ok(BspCollisionHull {
            planes: main_hull.planes.clone(),
            nodes: main_hull.nodes.clone(),
            node_id: self.collision_node_ids[index],
            node_count: self.collision_node_counts[index],
            mins: main_hull.mins,
            maxs: main_hull.maxs,
        })
    }
}

impl BspData {}

#[cfg(test)]
mod test {
    use super::*;
    use cgmath::Zero;

    #[test]
    fn test_hull_for_bounds() {
        let hull =
            BspCollisionHull::for_bounds(Vector3::zero(), Vector3::new(1.0, 1.0, 1.0)).unwrap();

        let empty_points = vec![
            // points strictly less than hull min should be empty
            Vector3::new(-1.0, -1.0, -1.0),
            // points strictly greater than hull max should be empty
            Vector3::new(2.0, 2.0, 2.0),
            // points in front of hull should be empty
            Vector3::new(2.0, 0.5, 0.5),
            // points behind hull should be empty
            Vector3::new(-1.0, 0.5, 0.5),
            // points left of hull should be empty
            Vector3::new(0.5, 2.0, 0.5),
            // points right of hull should be empty
            Vector3::new(0.5, -1.0, 0.5),
            // points above hull should be empty
            Vector3::new(0.5, 0.5, 2.0),
            // points below hull should be empty
            Vector3::new(0.5, 0.5, -1.0),
        ];

        for point in empty_points {
            assert_eq!(
                hull.contents_at_point(point).unwrap(),
                BspLeafContents::Empty
            );
        }

        let solid_points = vec![
            // center of the hull should be solid
            Vector3::new(0.5, 0.5, 0.5),
            // various interior corners should be solid
            Vector3::new(0.01, 0.01, 0.01),
            Vector3::new(0.99, 0.01, 0.01),
            Vector3::new(0.01, 0.99, 0.01),
            Vector3::new(0.01, 0.01, 0.99),
            Vector3::new(0.99, 0.99, 0.01),
            Vector3::new(0.99, 0.01, 0.99),
            Vector3::new(0.01, 0.99, 0.99),
            Vector3::new(0.99, 0.99, 0.99),
        ];

        for point in solid_points {
            assert_eq!(
                hull.contents_at_point(point).unwrap(),
                BspLeafContents::Solid
            );
        }
    }
}
