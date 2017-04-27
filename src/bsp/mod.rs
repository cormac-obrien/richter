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
// - Inline parse_edicts()?
// - Create project-wide Wad and WadEntry types
// - Replace index fields with direct references where possible

//! The binary space partitioning (BSP) tree is the central data structure in Quake maps.
//!
//! # Overview
//! The primary purpose of the BSP tree is to describe a hierarchy between the geometric facets
//! of a level. Each of the tree's nodes store a hyperplane in point-normal form, which allows
//! the leaf containing a desired point to be located in log(n) time. Additionally, each leaf
//! stores information about what can be seen from within that leaf, allowing most of the level
//! geometry to be excluded from collision detection and rendering.
//!
//! # Entities
//! The entity dictionary (*edict*) stores information about dynamic functionality in the level,
//! such as spawn points, dynamic lighting and moving geometry. The precise function of each entity
//! is defined in `progs.dat`, which is compiled QuakeC bytecode.
//!
//! # Planes
//! The planes are the primary method of navigation in the BSP tree. Given a point *p*, a plane
//! normal *n* and the distance *d* from the map origin, it is possible to calculate the point's
//! relative position to the plane with the formula
//!
//! > p &middot; n - d
//!
//! A positive result indicates that the point is in front of the plane, and a negative result is
//! behind. This allows quick traversal of a complex physical space to determine which leaf
//! contains a given point.
//!
//! # Visibility Lists
//! For each leaf *l* in the BSP tree, there exists a visibility list ('vislist') *v* that
//! describes which other leaves are visible from *l*. The vislists are stored as partially
//! run-length encoded bit vectors. For each byte in the vislist:
//!
//! - If the byte is nonzero (i.e. one or more bits set), it is interpreted as a list of boolean
//!   values (1 = visible, 0 = not visible).
//! - If the byte is zero, then the byte following it is interpreted as a count of zeroed bytes,
//!   and each of those zeroed bytes denotes 8 leaves that are not visible. Note that the count
//!   *includes the initial zero byte*.
//!
//! Thus, a packed vislist of the form
//!
//!     0x6B     0x00     0x0B     0x12
//!     01101011 00000000 00001011 00010010
//!
//! Expands to
//!
//!     01101011 00000000 00000000 00000000
//!     00000000 00000000 00000000 00000000
//!     00000000 00000000 00000000 00000000
//!     00001011 00010010
//!
//! The vislists are left packed in memory and only decompressed on a byte-by-byte basis when a
//! leaf needs to be rendered.
//!
//! # Nodes
//! The internal nodes of the tree are responsible for maintaining the hierarchy between
//! hyperplanes, containing the next level down in front and back of each plane.
//!
//! # Edges
//! The indexing system of the BSP format seems somewhat bizarre. Instead of the individual faces
//! being stored as a set of vertex indices, they are expressed as a set of edges&mdash;pairs of
//! indices forming a line segment. This results in nearly 100% redundancy in the vertex indices.
//! It turns out that this edge system is integral to Quake's software renderer.

mod bspload;

use std;
use std::collections::HashMap;
use std::fmt;
use std::io::Cursor;

use engine;
use gfx;
use glium::{Program, Texture2d, VertexBuffer};
use glium::backend::glutin_backend::GlutinFacade as Display;
use glium::index::{DrawCommandNoIndices, DrawCommandsNoIndicesBuffer, PrimitiveType};
use glium::uniforms::{MagnifySamplerFilter, MinifySamplerFilter, SamplerWrapFunction};
use load::{Load, LoadError};
use math;
use math::{Mat4, Vec3};
use num::FromPrimitive;
use regex::Regex;

pub const MAX_LIGHTSTYLE_COUNT: usize = 4;
pub const MAX_TEXTURE_FRAMES: usize = 10;

// Quake uses a different coordinate system than OpenGL:
//
//   | +z                 | +y
//   |                    |
//   |_____ +y Quake ->   |_____ +x OpenGL
//   /                    /
//  /                    /
// / +x                 / -z
//
// Quake  [x, y, z] <-> [-z, -x, y] OpenGL
// OpenGL [x, y, z] <-> [-y, z, -x] Quake

pub fn convert_coords(from: [f32; 3]) -> Vec3 {
    Vec3::new(-from[1], from[2], -from[0])
}

#[derive(Copy, Clone)]
struct BspVertex {
    position: [f32; 3],
    texcoord: [f32; 2],
}
implement_vertex!(BspVertex, position, texcoord);

#[derive(FromPrimitive)]
enum PlaneKind {
    X = 0,
    Y = 1,
    Z = 2,
    AnyX = 3,
    AnyY = 4,
    AnyZ = 5,
}

/// One of the hyperplanes partitioning the map.
struct Plane {
    normal: Vec3,
    distance: f32,
    kind: PlaneKind,
}

impl Plane {
    fn load<L>(data: &mut L) -> Result<Plane, LoadError>
        where L: Load
    {
        let mut normal = [0.0f32; 3];
        for i in 0..normal.len() {
            normal[i] = data.load_f32le(None)?;
        }
        let dist = data.load_f32le(None)?;
        let kind = data.load_i32le(Some(&(0..6)))?;
        Ok(Plane {
            normal: Vec3::from_components(normal),
            distance: dist,
            kind: PlaneKind::from_i32(kind).unwrap(),
        })
    }

    fn from_disk(p: &bspload::DiskPlane) -> Plane {
        Plane {
            normal: convert_coords(p.normal),
            distance: p.dist,
            kind: PlaneKind::from_i32(p.kind).unwrap(),
        }
    }
}

/// A named texture. Analogous to `texture_t`, see
/// https://github.com/id-Software/Quake/blob/master/WinQuake/gl_model.h#L76
struct Texture {
    name: String,
    tex: Texture2d,
    frame_count: usize,
    time_start: u32, // TODO: might change type based on usage w/ engine ticks
    time_end: u32,
    next: Option<usize>, // None if non-animated
    alt: Option<usize>,
}

impl Texture {
    fn from_disk(display: &Display, t: &bspload::DiskTexture) -> Texture {
        let mut len = 0;
        while len < t.name.len() && t.name[len] != 0 {
            len += 1;
        }
        let name = String::from_utf8(t.name[0..len].to_owned()).unwrap();

        if t.width <= 0 {
            panic!("Invalid texture width.");
        }

        if t.height <= 0 {
            panic!("Invalid texture height.");
        }

        let tex = engine::tex_from_indexed(display, &t.mipmaps[0], t.width as u32, t.height as u32);

        Texture {
            name: name,
            tex: tex,
            frame_count: 0,
            time_start: 0,
            time_end: 0,
            next: None,
            alt: None,
        }
    }
}

struct TextureInfo {
    s_vector: [f32; 3],
    s_offset: f32,
    t_vector: [f32; 3],
    t_offset: f32,
    tex_id: u32,
    animated: bool,
}

impl TextureInfo {
    fn load<L>(data: &mut L) -> Result<TextureInfo, LoadError>
        where L: Load
    {
        let mut s_vec = [0.0f32; 3];
        for i in 0..s_vec.len() {
            s_vec[i] = data.load_f32le(None)?;
        }
        let s_off = data.load_f32le(None)?;

        let mut t_vec = [0.0f32; 3];
        for i in 0..t_vec.len() {
            t_vec[i] = data.load_f32le(None)?;
        }
        let t_off = data.load_f32le(None)?;
        let tex_id = data.load_i32le(Some(&(0..)))?;
        let anim = data.load_i32le(Some(&(0..2)))?;

        Ok(TextureInfo {
            s_vector: s_vec,
            s_offset: s_off,
            t_vector: t_vec,
            t_offset: t_off,
            tex_id: tex_id as u32,
            animated: anim == 1,
        })
    }

    fn from_disk(s: &bspload::DiskTextureInfo) -> TextureInfo {
        let tex_id: u32;
        if s.tex_id < 0 {
            panic!("Invalid texture ID {}", s.tex_id);
        }
        tex_id = s.tex_id as u32;

        TextureInfo {
            s_vector: [s.vecs[0][0], s.vecs[0][1], s.vecs[0][2]],
            s_offset: s.vecs[0][3],
            t_vector: [s.vecs[1][0], s.vecs[1][1], s.vecs[1][2]],
            t_offset: s.vecs[1][3],
            tex_id: tex_id,
            animated: s.flags & 1 == 1,
        }
    }
}

/// Indicates which side of a hyperplane this face is on.
enum FaceSide {
    Front,
    Back,
}

/// Represents a physical facet of the map geometry.
struct Face {
    plane_id: usize,
    side: FaceSide,
    vertex_id: usize,
    vertex_count: usize,
    texinfo_id: usize,
    light_styles: [u8; MAX_LIGHTSTYLE_COUNT],
    draw_command: DrawCommandNoIndices,
}

/// A non-terminal node of the BSP tree.
struct Node {
    plane_id: usize,
    front: usize,
    back: usize,
    min: [i16; 3],
    max: [i16; 3],
    face_id: usize,
    face_count: usize,
}

impl Node {
    fn from_disk(n: &bspload::DiskNode) -> Node {
        Node {
            plane_id: n.plane_id as usize,
            front: n.children[0] as usize,
            back: n.children[1] as usize,
            min: n.mins.clone(),
            max: n.maxs.clone(),
            face_id: n.face_id as usize,
            face_count: n.face_count as usize,
        }
    }
}

enum LeafType {
    Normal,

    // Nothing is drawn.
    Void,

    // Screen tinted brown
    Water,

    // Screen tinted green, player takes minor damage
    Acid,

    // Screen tinted red, player takes major damage
    Lava,

    // Scrolling textures, no perspective applied
    Sky,
}

impl std::convert::From<i32> for LeafType {
    fn from(src: i32) -> LeafType {
        match src {
            -1 => LeafType::Normal,
            -2 => LeafType::Void,
            -4 => LeafType::Acid,
            -5 => LeafType::Lava,
            -6 => LeafType::Sky,
            _ => LeafType::Water,
        }
    }
}

impl fmt::Debug for LeafType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
               "{}",
               match *self {
                   LeafType::Normal => "Normal",
                   LeafType::Void => "Void",
                   LeafType::Water => "Water",
                   LeafType::Acid => "Acid",
                   LeafType::Lava => "Lava",
                   LeafType::Sky => "Sky",
               })
    }
}

/// A leaf node of the BSP tree.
struct Leaf {
    leaftype: LeafType,
    vis_offset: Option<usize>,
    min: [i16; 3],
    max: [i16; 3],
    face_id: usize,
    face_count: usize,
    sounds: [u8; 4],
}

/// A rough approximation of a BSP node used for preliminary collision detection.
struct ClipNode {
    plane_id: u32,
    front: i16,
    back: i16,
}

impl ClipNode {
    fn from_disk(c: &bspload::DiskClipNode) -> ClipNode {
        ClipNode {
            plane_id: c.plane_id as u32,
            front: c.children[0],
            back: c.children[1],
        }
    }
}

/// A relatively static part of the level geometry.
struct Model {
    min: [f32; 3],
    max: [f32; 3],
    origin: [f32; 3],
    roots: [i32; 4],
    leaf_count: i32,
    face_id: i32,
    face_count: i32,
}

impl Model {
    fn from_disk(m: &bspload::DiskModel) -> Model {
        Model {
            min: m.mins.clone(),
            max: m.maxs.clone(),
            origin: m.origin.clone(),
            roots: m.roots.clone(),
            leaf_count: m.leaf_count,
            face_id: m.face_id,
            face_count: m.face_count,
        }
    }
}

/// A BSP map.
pub struct Bsp {
    entities: Vec<HashMap<String, String>>,
    planes: Box<[Plane]>,
    textures: Box<[Texture]>,
    vertices: Box<[BspVertex]>,
    vertex_buffer: VertexBuffer<BspVertex>,
    visibility: Box<[u8]>,
    nodes: Box<[Node]>,
    texinfo: Box<[TextureInfo]>,
    faces: Box<[Face]>,
    lightmaps: Box<[u8]>,
    clipnodes: Box<[ClipNode]>,
    leaves: Box<[Leaf]>,
    facelist: Box<[u16]>,
    models: Box<[Model]>,
}

/// Parse the entity dictionaries out of their serialized form into a list of hashmaps.
fn parse_edicts(entstring: &str) -> Option<Vec<HashMap<String, String>>> {
    lazy_static! {
        // match strings of the form "KEY": "VALUE", capturing KEY and VALUE
        static ref KEYVAL_REGEX: Regex = Regex::new(r#"^"([a-z]+)"\s+"(.+)"$"#).unwrap();
    }

    let _lines: Vec<&str> = entstring.split('\n').collect();
    let mut lines = _lines.iter();
    let mut entities: Vec<HashMap<String, String>> = Vec::with_capacity(128);

    loop {
        match lines.next() {
            None => break,
            Some(l) => {
                match *l {
                    "\u{0}" => break,
                    "{" => (),
                    _ => {
                        error!("Entities must begin with '{{' (got {:?})", *l);
                        return None;
                    }
                }
            }
        }

        debug!("New entity");

        let mut entity: HashMap<String, String> = HashMap::with_capacity(8);

        while let Some(&line) = lines.next() {
            if line == "}" {
                entity.shrink_to_fit();
                entities.push(entity);
                break;
            }
            let groups = match KEYVAL_REGEX.captures(line) {
                None => {
                    error!("Invalid line in entity list: {}", line);
                    return None;
                }
                Some(g) => g,
            };

            let key = groups[1].to_string();

            // keys beginning with an underscore are treated as comments, see
            // https://github.com/id-Software/Quake/blob/master/QW/server/pr_edict.c#L843-L844
            if key.chars().next().unwrap() == '_' {
                continue;
            }

            let val = groups[2].to_string();

            debug!("\tInserting {{ \"{}\" : \"{}\" }}", key, val);
            entity.insert(key, val);
        }
    }

    entities.shrink_to_fit();
    Some(entities)
}

impl Bsp {
    pub fn load(display: &Display, data: &[u8]) -> Bsp {
        let src = bspload::DiskBsp::load(data).unwrap();
        let entities = parse_edicts(&src.entstring).unwrap();

        let planes: Vec<Plane> = src.planes
                                    .iter()
                                    .map(|p| Plane::from_disk(p))
                                    .collect();

        // Holds the sequence of frames for the texture's primary animation
        let mut anims: [Option<usize>; MAX_TEXTURE_FRAMES] = [None; MAX_TEXTURE_FRAMES];

        // Holds the sequence of frames for the texture's secondary animation
        let mut alt_anims: [Option<usize>; MAX_TEXTURE_FRAMES] = [None; MAX_TEXTURE_FRAMES];

        let mut textures: Vec<Texture> = src.textures
                                            .iter()
                                            .map(|t| Texture::from_disk(display, t))
                                            .collect();

        // Sequence texture animations. See
        // https://github.com/id-Software/Quake/blob/master/WinQuake/gl_model.c#L397
        for i in 0..src.textures.len() {
            // Skip if texture isn't animated or is already linked
            if !textures[i].name.starts_with("+") || textures[i].next.is_some() {
                continue;
            }

            println!("Sequencing {}", textures[i].name);

            for i in 0..MAX_TEXTURE_FRAMES {
                anims[i] = None;
                alt_anims[i] = None;
            }

            let mut frame_max = 0;
            let mut altframe_max = 0;

            const ASCII_0: usize = '0' as usize;
            const ASCII_9: usize = '9' as usize;
            const ASCII_A: usize = 'A' as usize;
            const ASCII_J: usize = 'J' as usize;
            const ASCII_a: usize = 'a' as usize;
            const ASCII_j: usize = 'j' as usize;

            let mut frame_char = textures[i].name.chars().nth(1).unwrap() as usize;
            match frame_char {
                ASCII_0...ASCII_9 => {
                    frame_max = frame_char - '0' as usize;
                    altframe_max = 0;
                    anims[frame_max] = Some(i);
                    frame_max += 1;
                }

                ASCII_A...ASCII_J | ASCII_a...ASCII_j => {
                    // capitalize if lowercase
                    if frame_char >= 'a' as usize && frame_char <= 'z' as usize {
                        frame_char -= 'a' as usize - 'A' as usize;
                    }

                    altframe_max = frame_char - 'A' as usize;
                    frame_max = 0;
                    alt_anims[altframe_max as usize] = Some(i);
                    altframe_max += 1;
                }

                _ => panic!("Bad frame specifier in animated texture ('{}')", frame_max),
            }

            for j in i + 1..textures.len() {
                let mut tex2 = &textures[j];

                if !textures[j].name.starts_with("+") ||
                   textures[j].name[2..] != textures[i].name[2..] {
                    continue;
                }

                println!("  {}", textures[j].name);

                let mut num = textures[j].name.chars().nth(1).unwrap() as usize;

                // capitalize if lowercase
                if num >= 'a' as usize && num <= 'z' as usize {
                    num -= 'a' as usize - 'A' as usize;
                }

                if num >= '0' as usize && num <= '9' as usize {
                    num -= '0' as usize;
                    anims[num as usize] = Some(j);
                    if num + 1 > frame_max {
                        frame_max = num + 1;
                    }
                } else if num >= 'A' as usize && num <= 'J' as usize {
                    num = num - 'A' as usize;
                    alt_anims[num as usize] = Some(j);
                    if num + 1 > altframe_max {
                        altframe_max = num + 1;
                    }
                } else {
                    panic!("Bad frame specifier in animated texture ('{}')", frame_max);
                }
            }

            const ANIM_CYCLE: usize = 2;

            for j in 0..frame_max as usize {
                let mut t2 = match anims[j] {
                    Some(t) => t,
                    None => panic!("Missing frame {} of {}", j, textures[i].name),
                };

                textures[t2].frame_count = frame_max * ANIM_CYCLE;
                textures[t2].time_start = (j * ANIM_CYCLE) as u32;
                textures[t2].time_end = ((j + 1) * ANIM_CYCLE) as u32;
                textures[t2].next = Some(anims[(j + 1) % frame_max].unwrap());

                if altframe_max != 0 {
                    textures[t2].alt = Some(alt_anims[0].unwrap());
                }
            }

            for j in 0..altframe_max as usize {
                let t2 = match alt_anims[j] {
                    Some(t) => t,
                    None => panic!("Missing frame {} of {}", j, textures[i].name),
                };

                textures[t2].frame_count = altframe_max * ANIM_CYCLE;
                textures[t2].time_start = (j * ANIM_CYCLE) as u32;
                textures[t2].time_end = ((j + 1) * ANIM_CYCLE) as u32;
                textures[t2].next = Some(alt_anims[(j + 1) % altframe_max].unwrap());

                if frame_max != 0 {
                    textures[t2].alt = Some(anims[0].unwrap());
                }
            }
        }

        let mut faces = Vec::with_capacity(src.faces.len());
        let mut vertices = Vec::new();
        for disk_face in src.faces.iter() {
            let first_edge = disk_face.edge_id as usize;
            let last_edge = first_edge + disk_face.edge_count as usize;

            let vertex_id = vertices.len();

            let texinfo = &src.texinfo[disk_face.texinfo as usize];
            let tex = &src.textures[texinfo.tex_id as usize];
            // -y z -x
            let s_vec = Vec3::new(-texinfo.vecs[0][1], texinfo.vecs[0][2], -texinfo.vecs[0][0]);
            let s_off = texinfo.vecs[0][3];
            let t_vec = Vec3::new(-texinfo.vecs[1][1], texinfo.vecs[1][2], -texinfo.vecs[1][0]);
            let t_off = texinfo.vecs[1][3];

            for e in first_edge..last_edge {
                let edge = src.surfedges[e];
                let index;
                if edge < 0 {
                    index = src.edges[-edge as usize].vertex_ids[1] as usize;
                } else {
                    index = src.edges[edge as usize].vertex_ids[0] as usize;
                }

                let vertex = &src.vertices[index];
                let pos = convert_coords(vertex.position);
                let s = (pos.dot(s_vec) + s_off) / tex.width as f32;
                let t = (pos.dot(t_vec) + t_off) / tex.height as f32;
                vertices.push(BspVertex {
                    position: *pos,
                    texcoord: [s, t],
                });
            }
            let vertex_count = vertices.len() - vertex_id;

            assert!(vertex_count >= 3);
            faces.push(Face {
                plane_id: disk_face.plane_id as usize,
                side: match disk_face.side {
                    0 => FaceSide::Front,
                    1 => FaceSide::Back,
                    s => panic!("Invalid face side value {}", s),
                },
                vertex_id: vertex_id,
                vertex_count: vertex_count,
                texinfo_id: disk_face.texinfo as usize,
                light_styles: disk_face.styles.clone(),
                draw_command: DrawCommandNoIndices {
                    count: vertex_count as std::os::raw::c_uint,
                    instance_count: 1,
                    first_index: vertex_id as std::os::raw::c_uint,
                    base_instance: 0,
                },
            });
        }

        let nodes: Vec<Node> = src.nodes
                                  .iter()
                                  .map(|n| Node::from_disk(n))
                                  .collect();

        let texinfo: Vec<TextureInfo> = src.texinfo
                                           .iter()
                                           .map(|t| TextureInfo::from_disk(t))
                                           .collect();

        let clipnodes: Vec<ClipNode> = src.clipnodes
                                          .iter()
                                          .map(|c| ClipNode::from_disk(c))
                                          .collect();

        let mut leaves = Vec::with_capacity(src.leaves.len());
        for disk_leaf in src.leaves.iter() {
            let face_id = disk_leaf.marksurf_id as usize;
            let face_count = face_id + disk_leaf.marksurf_count as usize;

            leaves.push(Leaf {
                leaftype: LeafType::from(disk_leaf.contents),
                vis_offset: match disk_leaf.vis_offset {
                    -1 => None,
                    x if x >= 0 => Some(x as usize),
                    x => panic!("Invalid vis offset: {}", x),
                },
                min: disk_leaf.mins.clone(),
                max: disk_leaf.maxs.clone(),
                face_id: face_id as usize,
                face_count: face_count as usize,
                sounds: disk_leaf.sounds.clone(),
            });
        }

        let vertex_buffer = VertexBuffer::new(display, &vertices).unwrap();

        let models: Vec<Model> = src.models
                                    .iter()
                                    .map(|m| Model::from_disk(m))
                                    .collect();

        Bsp {
            entities: entities,
            planes: planes.into_boxed_slice(),
            textures: textures.into_boxed_slice(),
            vertices: vertices.into_boxed_slice(),
            vertex_buffer: vertex_buffer,
            visibility: src.visibility.clone(),
            nodes: nodes.into_boxed_slice(),
            texinfo: texinfo.into_boxed_slice(),
            faces: faces.into_boxed_slice(),
            lightmaps: src.lightmaps.clone(),
            clipnodes: clipnodes.into_boxed_slice(),
            leaves: leaves.into_boxed_slice(),
            facelist: src.marksurfaces.clone(),
            models: models.into_boxed_slice(),
        }
    }

    pub fn draw_naive(&self, display: &Display, view_matrix: &Mat4) {
        let program = Program::new(display, gfx::get_bsp_shader_source()).unwrap();
        let mut target = display.draw();
        use glium::Surface;
        target.clear_color(0.0, 0.0, 0.0, 1.0);
        target.clear_depth(0.0);
        let (w, h) = target.get_dimensions();

        let mut first_face = 0;
        while first_face < self.faces.len() {
            let surface_id = self.faces[first_face].texinfo_id;
            let mut face_count = 0;

            while self.faces[first_face + face_count].texinfo_id == surface_id {
                face_count += 1;

                if first_face + face_count >= self.faces.len() {
                    break;
                }
            }

            let mut commands = Vec::new();
            for i in 0..face_count {
                commands.push(self.faces[first_face + i].draw_command.clone());
            }
            commands.shrink_to_fit();

            let command_buffer = DrawCommandsNoIndicesBuffer::empty(display, commands.len())
                                     .unwrap();
            command_buffer.write(commands.as_slice());

            let surf = &self.texinfo[surface_id as usize];
            let base_tex = &self.textures[surf.tex_id as usize];

            // Find proper frame for animated textures. See R_TextureAnimation,
            // https://github.com/id-Software/Quake/blob/master/WinQuake/r_surf.c#L213
            let tex = match base_tex.next {
                Some(bt) => {
                    let time = 0; // TODO: set relative to engine clock
                    let mut t = bt;

                    while self.textures[t].time_start > time || self.textures[t].time_end <= time {
                        t = match self.textures[t].next {
                            Some(nt) => nt,
                            None => panic!("broken cycle"),
                        };
                    }

                    &self.textures[t]
                }

                None => base_tex,
            };

            let uniforms = uniform! {
                perspective: *Mat4::perspective(w as f32, h as f32, math::PI / 2.0),
                view: **view_matrix,
                world: *Mat4::identity(),
                tex: tex.tex.sampled()
                            .magnify_filter(MagnifySamplerFilter::Nearest)
                            .minify_filter(MinifySamplerFilter::NearestMipmapNearest)
                            .wrap_function(SamplerWrapFunction::Repeat),
            };

            target.draw(&self.vertex_buffer,
                        command_buffer.with_primitive_type(PrimitiveType::TriangleFan),
                        &program,
                        &uniforms,
                        &gfx::get_draw_parameters())
                  .unwrap();

            first_face += face_count;
        }

        target.finish().unwrap();
    }

    fn find_leaf<V>(&self, point: V) -> &Leaf
        where V: AsRef<Vec3>
    {
        let mut node_index = 0;

        while node_index & (1 << 15) == 0 {
            let node = &self.nodes[node_index];
            let plane = &self.planes[node.plane_id as usize];

            if point.as_ref().dot(plane.normal) - plane.distance < 0.0 {
                node_index = node.front as usize;
            } else {
                node_index = node.back as usize;
            }
        }

        &self.leaves[node_index]
    }
}
