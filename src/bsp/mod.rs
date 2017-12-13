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

use std::collections::HashSet;
use std::error::Error;
use std::fmt;
use std::ops::Deref;
use std::rc::Rc;

use math::Hyperplane;
use math::HyperplaneSide;
use math::LinePlaneIntersect;
use world::Trace;
use world::TraceStart;
use world::TraceEnd;

use chrono::Duration;
use cgmath::InnerSpace;
use cgmath::Vector3;

pub use self::load::load;

// this is 4 in the original source, but the 4th hull is never used.
const MAX_HULLS: usize = 3;

pub const MAX_LIGHTSTYLES: usize = 4;
pub const MAX_SOUNDS: usize = 4;
const MIPLEVELS: usize = 4;
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
    children: [BspRenderNodeChild; 2],
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
enum BspCollisionNodeChild {
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
            mins,
            maxs
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
                            near_side,
                            near_n
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
                &BspCollisionNodeChild::Contents(c) => {
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
    planes: Rc<Box<[Hyperplane]>>,
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
            nodes: main_hull.nodes.clone(),
            node_id: self.collision_node_ids[index],
            node_count: self.collision_node_counts[index],
            mins: main_hull.mins,
            maxs: main_hull.maxs,
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

            let child = &node.children[plane.point_side(pos_vec) as usize];

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

#[cfg(test)]
mod test {
    use super::*;
    use cgmath::Zero;

    #[test]
    fn test_hull_for_bounds() {
        let hull = BspCollisionHull::for_bounds(Vector3::zero(), Vector3::new(1.0, 1.0, 1.0))
            .unwrap();

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
