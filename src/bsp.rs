// Copyright © 2016 Cormac O'Brien
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

use std;
use std::collections::HashMap;
use std::convert::From;
use std::fmt;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::process::exit;

use engine;
use gfx;
use gfx::Vertex;
use glium;
use glium::{IndexBuffer, Program, Texture2d, VertexBuffer};
use glium::backend::glutin_backend::GlutinFacade as Display;
use glium::index::{DrawCommandIndices, DrawCommandsIndicesBuffer, PrimitiveType};
use glium::uniforms::{MagnifySamplerFilter, MinifySamplerFilter, SamplerWrapFunction};
use load::Load;
use math;
use math::{Mat4, Vec3};
use regex::Regex;

const VERSION: i32 = 29;

const ENTITY_ENTRY: usize = 0;
const PLANE_ENTRY: usize = 1;
const MIPTEX_ENTRY: usize = 2;
const VERTEX_ENTRY: usize = 3;
const VISLIST_ENTRY: usize = 4;
const NODE_ENTRY: usize = 5;
const SURFACE_ENTRY: usize = 6;
const FACE_ENTRY: usize = 7;
const LIGHTMAP_ENTRY: usize = 8;
const CLIPNODE_ENTRY: usize = 9;
const LEAF_ENTRY: usize = 10;
const FACELIST_ENTRY: usize = 11;
const EDGE_ENTRY: usize = 12;
const EDGELIST_ENTRY: usize = 13;
const MODEL_ENTRY: usize = 14;

const PLANE_SIZE: usize = 20;
const NODE_SIZE: usize = 24;
const LEAF_SIZE: usize = 28;
const SURFACE_SIZE: usize = 40;
const FACE_SIZE: usize = 20;
const CLIPNODE_SIZE: usize = 8;
const FACELIST_SIZE: usize = 2;
const EDGE_SIZE: usize = 4;
const EDGELIST_SIZE: usize = 4;
const MODEL_SIZE: usize = 64;
const VERTEX_SIZE: usize = 12;
const TEX_NAME_MAX: usize = 16;

// As defined in bspfile.h
const MAX_MODELS: usize = 256;
const MAX_LEAVES: usize = 32767;
const MAX_BRUSHES: usize = 4096;
const MAX_ENTITIES: usize = 1024;
const MAX_ENTSTRING: usize = 65536;
const MAX_PLANES: usize = 8192;
const MAX_NODES: usize = 32767;
const MAX_CLIPNODES: usize = 32767;
const MAX_VERTICES: usize = 65535;
const MAX_FACES: usize = 65535;
const MAX_MARKSURFACES: usize = 65535;
const MAX_SURFACES: usize = 4096;
const MAX_EDGES: usize = 256000;
const MAX_SURFEDGES: usize = 512000;
const MAX_TEXTURES: usize = 0x200000;
const MAX_LIGHTMAP: usize = 0x100000;
const MAX_VISLIST: usize = 0x100000;


struct Entry {
    offset: usize,
    size: usize,
}

enum PlaneKind {
    AxialX,
    AxialY,
    AxialZ,
    NonAxialX,
    NonAxialY,
    NonAxialZ,
}

/// One of the hyperplanes partitioning the map.
struct Plane {
    normal: Vec3,
    offset: f32,
    kind: PlaneKind,
}

struct BoundsFloat {
    min: [f32; 3],
    max: [f32; 3],
}

struct BoundsShort {
    min: [i16; 3],
    max: [i16; 3],
}

impl fmt::Debug for BoundsShort {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "min: {{{}, {}, {}}}, max: {{{}, {}, {}}}",
            self.min[0], self.min[1], self.min[2],
            self.max[0], self.max[1], self.max[2])
    }
}

/// A named texture.
struct Texture {
    name: String,
    tex: Texture2d,
    w: usize,
    h: usize,
}

struct Surface {
    s_vector: [f32; 3],
    s_offset: f32,
    t_vector: [f32; 3],
    t_offset: f32,
    tex_id: u32,
    animated: bool,
}

/// Indicates which side of a hyperplane this face is on.
enum FaceSide {
    Front,
    Back,
}

impl fmt::Debug for FaceSide {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", match *self {
           FaceSide::Front => "Front",
           FaceSide::Back => "Back",
        })
    }
}

enum FaceLightKind {
    Normal,
    FastPulse,
    SlowPulse,
    Custom(u8),
    Disabled,
}

impl fmt::Debug for FaceLightKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", match *self {
            FaceLightKind::Normal => "Normal",
            FaceLightKind::FastPulse => "Fast Pulse",
            FaceLightKind::SlowPulse => "Slow Pulse",
            FaceLightKind::Disabled => "Disabled",
            FaceLightKind::Custom(_) => "Custom",
        })
    }
}

/// Represents a physical facet of the map geometry.
struct Face {
    plane_id: u16,
    side: FaceSide,
    index_first: u32,
    index_count: u16,
    surface_id: u16,
    light_kind: FaceLightKind,
    base_light: u8,
    misc_light: [u8; 2],
    lightmap_off: i32,
    draw_command: DrawCommandIndices,
}

impl fmt::Debug for Face {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Plane #: {} | Dir'n: {:?} | First index: {} | Index count: {:?} | \
                   Surface ID: {} | Light kind: {:?} | Light level: {:?} | \
                   Light info: {}, {} | Lightmap offset: {:?}",
                 self.plane_id, self.side, self.index_first, self.index_count, self.surface_id,
                 self.light_kind, self.base_light, self.misc_light[0], self.misc_light[1],
                 self.lightmap_off)
    }
}

/// A non-terminal node of the BSP tree.
struct InternalNode {
    plane_id: i32,
    front: u16,
    back: u16,
    bounds: BoundsShort,
    face_id: u16,
    face_count: u16,
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

impl From<i32> for LeafType {
    fn from(src: i32) -> LeafType {
        match src {
            -1 => LeafType::Normal,
            -2 => LeafType::Void,
            -4 => LeafType::Acid,
            -5 => LeafType::Lava,
            -6 => LeafType::Sky,
            _ => LeafType::Water
        }
    }
}

impl fmt::Debug for LeafType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", match *self {
            LeafType::Normal => "Normal",
            LeafType::Void => "Void",
            LeafType::Water => "Water",
            LeafType::Acid => "Acid",
            LeafType::Lava => "Lava",
            LeafType::Sky => "Sky",
        })
    }
}

struct LeafSound {
    water: u8,
    sky: u8,
    acid: u8,
    lava: u8,
}

impl fmt::Debug for LeafSound {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Water@{:X}, Sky@{:X}, Acid@{:X}, Lava@{:X}",
            self.water, self.sky, self.acid, self.lava)
    }
}

/// A leaf node of the BSP tree.
struct Leaf {
    leaftype: LeafType,
    vislist_id: i32,
    bounds: BoundsShort,
    facelist_id: u16,
    face_count: u16,
    sound: LeafSound,
}

impl fmt::Debug for Leaf {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Type: {:?} | Vislist index: {} | Bounds: {:?} | Facelist index: {} | \
                   Face count: {} | Sound: {:?}", self.leaftype, self.vislist_id, self.bounds,
                    self.facelist_id, self.face_count, self.sound)
    }
}

enum Node {
    Internal(InternalNode),
    Leaf(Leaf),
}

/// A rough approximation of a BSP node used for preliminary collision detection.
struct ClipNode {
    plane_id: u32,
    front: i16,
    back: i16,
}

/// A relatively static part of the level geometry.
struct Model {
    bounds: BoundsFloat,
    origin: [f32; 3],
    node_ids: [i32; 4],
    leaf_count: i32,
    face_id: i32,
    face_count: i32,
}

/// A BSP map.
pub struct Bsp {
    entities: Vec<HashMap<String, String>>,
    planes: Vec<Plane>,
    textures: Vec<Texture>,
    vertices: VertexBuffer<Vertex>,
    indices: IndexBuffer<u16>,
    vislists: Vec<u8>,
    nodes: Vec<InternalNode>,
    surfaces: Vec<Surface>,
    faces: Vec<Face>,
    lightmaps: Vec<u8>,
    clipnodes: Vec<ClipNode>,
    leaves: Vec<Leaf>,
    facelist: Vec<u16>,
    models: Vec<Model>,
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
            Some(l) => match *l {
                "\u{0}" => break,
                "{" => (),
                _ => {
                    error!("Entities must begin with '{{' (got {:?})", *l);
                    return None;
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

            let key = groups.at(1).unwrap().to_string();

            // keys beginning with an underscore are treated as comments, see
            // https://github.com/id-Software/Quake/blob/master/QW/server/pr_edict.c#L843-L844
            if key.chars().next().unwrap() == '_' {
                continue;
            }

            let val = groups.at(2).unwrap().to_string();

            debug!("\tInserting {{ \"{}\" : \"{}\" }}", key, val);
            entity.insert(key, val);
        }
    }

    entities.shrink_to_fit();
    Some(entities)
}

impl Bsp {
    /// Load the serialized entity dictionaries and parse them into edicts.
    fn load_entities<R>(entry: &Entry, bspreader: &mut BufReader<&mut R>) -> Vec<HashMap<String, String>> where R: Read + Seek {
        bspreader.seek(SeekFrom::Start(entry.offset as u64)).unwrap();
        let entstring: String = {
            let mut _entstring: Vec<u8> = Vec::with_capacity(MAX_ENTSTRING);
            bspreader.read_until(0x00, &mut _entstring).unwrap();
            String::from_utf8(_entstring).unwrap()
        };

        assert!(bspreader.seek(SeekFrom::Current(0)).unwrap() == bspreader.seek(SeekFrom::Start((entry.offset + entry.size) as u64)).unwrap());

        match parse_edicts(&entstring) {
            None => {
                error!("Couldn't parse entity dictionary.");
                exit(1);
            }
            Some(e) => e,
        }
    }

    fn load_planes<R>(entry: &Entry, bspreader: &mut BufReader<&mut R>) -> Vec<Plane>
            where R: Read + Seek {
        bspreader.seek(SeekFrom::Start(entry.offset as u64)).unwrap();
        assert!(entry.size % PLANE_SIZE == 0);
        let plane_count = entry.size / PLANE_SIZE;
        let mut _planes: Vec<Plane> = Vec::with_capacity(plane_count);
        for _ in 0..plane_count {
            _planes.push(Plane {
                normal: Vec3::new(bspreader.load_f32le(), bspreader.load_f32le(), bspreader.load_f32le()),
                offset: bspreader.load_f32le(),
                kind: match bspreader.load_i32le() {
                    0 => PlaneKind::AxialX,
                    1 => PlaneKind::AxialY,
                    2 => PlaneKind::AxialZ,
                    3 => PlaneKind::NonAxialX,
                    4 => PlaneKind::NonAxialY,
                    5 => PlaneKind::NonAxialZ,
                    _ => panic!("Unrecognized plane kind"),
                },
            });
        }
        assert!(bspreader.seek(SeekFrom::Current(0)).unwrap() == bspreader.seek(SeekFrom::Start((entry.offset + entry.size) as u64)).unwrap());
        _planes
    }

    fn load_textures<R>(display: &Display, entry: &Entry, bspreader: &mut BufReader<&mut R>) -> Vec<Texture> where R: Read + Seek {
        bspreader.seek(SeekFrom::Start(entry.offset as u64)).unwrap();
        let tex_count = match bspreader.load_i32le() {
            t if t <= 0 => panic!("Invalid texture count {}", t),
            t => t as usize
        };

        let mut tex_offsets = Vec::with_capacity(tex_count);
        for _ in 0..tex_count {
            tex_offsets.push(match bspreader.load_i32le() {
                -1 => continue,
                t if t < 0 => panic!("Invalid texture count {}", t),
                t => t as usize
            });
        }

        let mut textures = Vec::with_capacity(tex_count);

        for off in tex_offsets {
            bspreader.seek(SeekFrom::Start((entry.offset + off) as u64)).unwrap();

            let mut bytes = [0u8; TEX_NAME_MAX];
            bspreader.read(&mut bytes).unwrap();

            let mut len = 0;
            while bytes[len] != b'\0' {
                len += 1
            }
            assert!(len != 0);
            let texname = String::from_utf8(bytes[..len].to_vec()).unwrap();

            debug!("Loading \"{}\"", texname);

            let texwidth = bspreader.load_u32le() as usize;  assert!(texwidth % 8 == 0);
            let texheight = bspreader.load_u32le() as usize; assert!(texheight % 8 == 0);
            let texoff = bspreader.load_u32le();

            // discard other mipmap offsets, we'll let the GPU generate the mipmaps
            for _ in 0..3 {
                bspreader.load_u32le();
            }

            bspreader.seek(SeekFrom::Current(texoff as i64)).unwrap();
            let mut indices = Vec::with_capacity(texwidth * texheight);
            bspreader.take((texwidth * texheight) as u64).read_to_end(&mut indices).unwrap();
            assert!(indices.len() == (texwidth * texheight) as usize);
            let tex = engine::tex_from_indexed(display, &indices, texwidth as u32, texheight as u32);
            textures.push(Texture {
                name: texname,
                tex: tex,
                w: texwidth,
                h: texheight,
            });
        }

        textures
    }

    fn load_vertices<R>(entry: &Entry, bspreader: &mut BufReader<&mut R>) -> Vec<Vertex> where R: Read + Seek {
        bspreader.seek(SeekFrom::Start(entry.offset as u64)).unwrap();
        assert!(entry.size % (std::mem::size_of::<f32>() * 3) == 0);
        let vertex_count = entry.size / VERTEX_SIZE;

        let mut vertices = Vec::with_capacity(vertex_count);
        for _ in 0..vertex_count {
            // Quake uses a different coordinate system than OpenGL:
            //
            //   | +z
            //   |
            //   |____ +y
            //   /
            //  /
            // / +x
            let z = -bspreader.load_f32le();
            let x = -bspreader.load_f32le();
            let y = bspreader.load_f32le();
            vertices.push(Vertex { pos: [x, y, z], });
            /*
            vertices.push(Vertex {
                pos: [bspreader.load_f32le(), bspreader.load_f32le(), bspreader.load_f32le()],
            });
            */
        }

        for v in vertices.iter() {
            debug!("{}", v);
        }

        assert!(bspreader.seek(SeekFrom::Current(0)).unwrap() == bspreader.seek(SeekFrom::Start((entry.offset + entry.size) as u64)).unwrap());
        vertices
    }

    fn load_vislists<R>(entry: &Entry, bspreader: &mut BufReader<&mut R>) -> Vec<u8> where R: Read + Seek {
        bspreader.seek(SeekFrom::Start(entry.offset as u64)).unwrap();
        let mut vislists: Vec<u8> = Vec::with_capacity(entry.size);
        bspreader.take(entry.size as u64).read_to_end(&mut vislists).unwrap();
        assert!(bspreader.seek(SeekFrom::Current(0)).unwrap() == bspreader.seek(SeekFrom::Start((entry.offset + entry.size) as u64)).unwrap());
        vislists
    }

    fn load_nodes<R>(entry: &Entry, bspreader: &mut BufReader<&mut R>) -> Vec<InternalNode> where R: Read + Seek {
        bspreader.seek(SeekFrom::Start(entry.offset as u64)).unwrap();
        let node_count = entry.size / NODE_SIZE;
        let mut nodes = Vec::with_capacity(node_count);
        for _ in 0..node_count {
            nodes.push(InternalNode {
                plane_id: bspreader.load_i32le(),
                front: bspreader.load_u16le(),
                back: bspreader.load_u16le(),
                bounds: BoundsShort {
                    min: [bspreader.load_i16le(), bspreader.load_i16le(), bspreader.load_i16le()],
                    max: [bspreader.load_i16le(), bspreader.load_i16le(), bspreader.load_i16le()],
                },
                face_id: bspreader.load_u16le(),
                face_count: bspreader.load_u16le(),
            });
        }
        assert!(bspreader.seek(SeekFrom::Current(0)).unwrap() == bspreader.seek(SeekFrom::Start((entry.offset + entry.size) as u64)).unwrap());
        nodes
    }

    fn load_surfaces<R>(entry: &Entry, bspreader: &mut BufReader<&mut R>) -> Vec<Surface> where R: Read + Seek {
        bspreader.seek(SeekFrom::Start(entry.offset as u64)).unwrap();
        assert!(entry.size % SURFACE_SIZE == 0);

        let surface_count = entry.size / SURFACE_SIZE;
        let mut surfaces = Vec::with_capacity(surface_count);
        for _ in 0..surface_count {
            surfaces.push(Surface {
                s_vector: {
                    let z = -bspreader.load_f32le();
                    let x = -bspreader.load_f32le();
                    let y =  bspreader.load_f32le();
                    [x, y, z]
                },
                s_offset: bspreader.load_f32le(),
                t_vector: {
                    let z = -bspreader.load_f32le();
                    let x = -bspreader.load_f32le();
                    let y =  bspreader.load_f32le();
                    [x, y, z]
                },
                t_offset: bspreader.load_f32le(),
                tex_id: bspreader.load_u32le(),
                animated: bspreader.load_u32le() != 0,
            });
        }

        assert!(bspreader.seek(SeekFrom::Current(0)).unwrap() == bspreader.seek(SeekFrom::Start((entry.offset + entry.size) as u64)).unwrap());
        surfaces
    }

    fn load_faces<R>(entry: &Entry, bspreader: &mut BufReader<&mut R>) -> Vec<Face> where R: Read + Seek {
        bspreader.seek(SeekFrom::Start(entry.offset as u64)).unwrap();
        assert!(entry.size % FACE_SIZE == 0);

        let face_count = entry.size / FACE_SIZE;
        let mut faces = Vec::with_capacity(face_count);
        for i in 0..face_count {
            let plane_id = bspreader.load_u16le();
            let side = match bspreader.load_u16le() {
                0 => FaceSide::Front,
                _ => FaceSide::Back,
            };

            let index_first = match bspreader.load_i32le() {
                e if e < 0 => panic!("Edge index below zero. (Face at index {}, offset 0x{:X})", i, bspreader.seek(SeekFrom::Current(0)).unwrap()),
                e => e as u32,
            };
            let index_count = bspreader.load_u16le();
            let draw_command = DrawCommandIndices {
                count: index_count as u32,
                instance_count: 1,
                first_index: index_first as u32,
                base_vertex: 0,
                base_instance: 0,
            };

            let surface_id = bspreader.load_u16le();
            let light_kind = match bspreader.load_u8() {
                0 => FaceLightKind::Normal,
                1 => FaceLightKind::FastPulse,
                2 => FaceLightKind::SlowPulse,
                l @ 3...64 => FaceLightKind::Custom(l),
                255 => FaceLightKind::Disabled,
                _ => FaceLightKind::Disabled,
            };

            let base_light = bspreader.load_u8();
            let misc_light = [bspreader.load_u8(), bspreader.load_u8()];
            let lightmap_off = bspreader.load_i32le();

            let face = Face {
                plane_id: plane_id,
                side: side,
                index_first: index_first,
                index_count: index_count,
                surface_id: surface_id,
                light_kind: light_kind,
                base_light: base_light,
                misc_light: misc_light,
                lightmap_off: lightmap_off,
                draw_command: draw_command,
            };
            debug!("Face {}: {:?}", i, face);
            faces.push(face);
        }
        assert!(bspreader.seek(SeekFrom::Current(0)).unwrap() == bspreader.seek(SeekFrom::Start((entry.offset + entry.size) as u64)).unwrap());
        faces.sort_by(|f1, f2| f1.surface_id.cmp(&f2.surface_id));
        faces
    }

    fn load_lightmaps<R>(entry: &Entry, bspreader: &mut BufReader<&mut R>) -> Vec<u8> where R: Read + Seek {
        bspreader.seek(SeekFrom::Start(entry.offset as u64)).unwrap();
        let mut lightmaps = Vec::with_capacity(entry.size);
        bspreader.take(entry.size as u64).read_to_end(&mut lightmaps).unwrap();
        assert!(bspreader.seek(SeekFrom::Current(0)).unwrap() == bspreader.seek(SeekFrom::Start((entry.offset + entry.size) as u64)).unwrap());
        lightmaps
    }

    fn load_clipnodes<R>(entry: &Entry, bspreader: &mut BufReader<&mut R>) -> Vec<ClipNode> where R: Read + Seek {
        bspreader.seek(SeekFrom::Start(entry.offset as u64)).unwrap();
        assert!(entry.size % CLIPNODE_SIZE == 0);

        let clipnode_count = entry.size / CLIPNODE_SIZE;
        let mut clipnodes = Vec::with_capacity(clipnode_count);
        for _ in 0..clipnode_count {
            clipnodes.push(ClipNode {
                plane_id: bspreader.load_u32le(),
                front: bspreader.load_i16le(),
                back: bspreader.load_i16le(),
            });
        }
        assert!(bspreader.seek(SeekFrom::Current(0)).unwrap() == bspreader.seek(SeekFrom::Start((entry.offset + entry.size) as u64)).unwrap());
        clipnodes
    }

    fn load_leaves<R>(entry: &Entry, bspreader: &mut BufReader<&mut R>) -> Vec<Leaf> where R: Read + Seek {
        bspreader.seek(SeekFrom::Start(entry.offset as u64)).unwrap();
        assert!(entry.size % LEAF_SIZE == 0);

        let leaf_count = entry.size / LEAF_SIZE;
        assert!(leaf_count < MAX_LEAVES);

        let mut leaves = Vec::with_capacity(leaf_count);

        // Leaf 0 represents all space outside the level geometry and is not drawn
        leaves.push(Leaf {
            leaftype: LeafType::Void,
            vislist_id: -1,
            bounds: BoundsShort{
                min: [0, 0, 0],
                max: [0, 0, 0],
            },
            facelist_id: 0,
            face_count: 0,
            sound: LeafSound {
                water: 0,
                sky: 0,
                acid: 0,
                lava: 0,
            },
        });

        for i in 0..leaf_count {
            let leaf = Leaf {
                leaftype: LeafType::from(bspreader.load_i32le()),
                vislist_id: bspreader.load_i32le(),
                bounds: BoundsShort {
                    min: [bspreader.load_i16le(), bspreader.load_i16le(), bspreader.load_i16le()],
                    max: [bspreader.load_i16le(), bspreader.load_i16le(), bspreader.load_i16le()],
                },
                facelist_id: bspreader.load_u16le(),
                face_count: bspreader.load_u16le(),
                sound: LeafSound {
                    water: bspreader.load_u8(),
                    sky: bspreader.load_u8(),
                    acid: bspreader.load_u8(),
                    lava: bspreader.load_u8(),
                }
            };
            debug!("Leaf {}: {:?}", i, leaf);
            leaves.push(leaf);
        }
        assert!(bspreader.seek(SeekFrom::Current(0)).unwrap() == bspreader.seek(SeekFrom::Start((entry.offset + entry.size) as u64)).unwrap());
        leaves
    }

    fn load_facelist<R>(entry: &Entry, bspreader: &mut BufReader<&mut R>) -> Vec<u16> where R: Read + Seek {
        bspreader.seek(SeekFrom::Start(entry.offset as u64)).unwrap();
        assert!(entry.size % FACELIST_SIZE == 0);

        let facelist_count = entry.size / FACELIST_SIZE;
        let mut facelist = Vec::with_capacity(facelist_count);
        for _ in 0..facelist_count {
            facelist.push(bspreader.load_u16le());
        }
        assert!(bspreader.seek(SeekFrom::Current(0)).unwrap() == bspreader.seek(SeekFrom::Start((entry.offset + entry.size) as u64)).unwrap());
        facelist
    }

    fn load_edges<R>(entry: &Entry, bspreader: &mut BufReader<&mut R>) -> Vec<(u16, u16)> where R: Read + Seek {
        bspreader.seek(SeekFrom::Start(entry.offset as u64)).unwrap();
        assert!(entry.size % EDGE_SIZE == 0);
        let index_count = entry.size / EDGE_SIZE;
        let mut edges = Vec::with_capacity(index_count);
        for i in 0..index_count {
            let edge = (bspreader.load_u16le(), bspreader.load_u16le());
            debug!("Edge {}: {} -> {}", i, edge.0, edge.1);
            edges.push(edge);
        }

        assert!(edges[0] == (0, 0));
        debug!("Edge count is {}", edges.len());
        assert!(bspreader.seek(SeekFrom::Current(0)).unwrap() == bspreader.seek(SeekFrom::Start((entry.offset + entry.size) as u64)).unwrap());
        edges
    }

    fn load_edgelist<R>(entry: &Entry, bspreader: &mut BufReader<&mut R>) -> Vec<i32> where R: Read + Seek {
        bspreader.seek(SeekFrom::Start(entry.offset as u64)).unwrap();
        assert!(entry.size % EDGELIST_SIZE == 0);

        let edgelist_count = entry.size / EDGELIST_SIZE;
        let mut edgelist = Vec::with_capacity(edgelist_count);

        for i in 0..edgelist_count {
            let edge_entry = bspreader.load_i32le();
            debug!("Edge table {}: {}", i, edge_entry);
            edgelist.push(edge_entry);
        }

        let expected = bspreader.seek(SeekFrom::Current(0)).unwrap();
        debug!("Expected {}", expected);

        let actual = bspreader.seek(SeekFrom::Start((entry.offset + entry.size) as u64)).unwrap();
        debug!("Got {}", actual);
        assert!(expected == actual);
        edgelist
    }

    fn load_models<R>(entry: &Entry, bspreader: &mut BufReader<&mut R>) -> Vec<Model> where R: Read + Seek {
        bspreader.seek(SeekFrom::Start(entry.offset as u64)).unwrap();
        assert!(entry.size % MODEL_SIZE == 0);

        let model_count = entry.size / MODEL_SIZE;
        assert!(model_count < MAX_MODELS);

        let mut models = Vec::with_capacity(model_count);
        for _ in 0..model_count {
            models.push(Model {
                bounds: BoundsFloat {
                    min: [bspreader.load_f32le(), bspreader.load_f32le(), bspreader.load_f32le()],
                    max: [bspreader.load_f32le(), bspreader.load_f32le(), bspreader.load_f32le()],
                },
                origin: [bspreader.load_f32le(), bspreader.load_f32le(), bspreader.load_f32le()],
                node_ids: [bspreader.load_i32le(), bspreader.load_i32le(),
                           bspreader.load_i32le(), bspreader.load_i32le()],
                leaf_count: bspreader.load_i32le(),
                face_id: bspreader.load_i32le(),
                face_count: bspreader.load_i32le(),
            });
        }
        assert!(bspreader.seek(SeekFrom::Current(0)).unwrap() == bspreader.seek(SeekFrom::Start((entry.offset + entry.size) as u64)).unwrap());
        models
    }

    pub fn load<R>(display: &Display, bspfile: &mut R) -> Bsp where R: Read + Seek {
        let mut bspreader = BufReader::new(bspfile);
        let version = bspreader.load_i32le();
        assert_eq!(version, VERSION);

        let entries: Vec<Entry> = {
            let mut _entries = Vec::with_capacity(15);
            for _ in 0..15 {
                _entries.push(Entry {
                    offset: match bspreader.load_i32le() {
                        o if o < 0 => panic!("Invalid offset ({})", o),
                        o => o as usize,
                    },

                    size: match bspreader.load_i32le() {
                        s if s < 0 => panic!("Invalid size value {}", s),
                        s => s as usize,
                    },
                })
            }
            _entries
        };

        let edges = Bsp::load_edges(&entries[EDGE_ENTRY], &mut bspreader);
        let edgelist = Bsp::load_edgelist(&entries[EDGELIST_ENTRY], &mut bspreader);
        /*
        let indices: Vec<u16> = edgelist.iter().map(|i|
            if *i < 0 {
                edges[-*i as usize].1
            } else {
                edges[*i as usize].0
            }).collect();
        */

        let mut indices = Vec::with_capacity(edgelist.len());
        for (i, e) in edgelist.iter().enumerate() {
            if *e < 0 {
                let x = edges[-*e as usize].1;
                debug!("Index {}: {}", i, -*e);
                indices.push(x);
            } else {
                let x = edges[*e as usize].0;
                debug!("Index {}: {}", i, *e);
                indices.push(x);
            }
        }

        let result = Bsp {
            entities: Bsp::load_entities(&entries[ENTITY_ENTRY], &mut bspreader),
            planes: Bsp::load_planes(&entries[PLANE_ENTRY], &mut bspreader),
            textures: Bsp::load_textures(&display, &entries[MIPTEX_ENTRY], &mut bspreader),
            vertices: VertexBuffer::new(display, &Bsp::load_vertices(&entries[VERTEX_ENTRY], &mut bspreader)).unwrap(),
            indices: IndexBuffer::new(display, PrimitiveType::TriangleFan, &indices).unwrap(),
            vislists: Bsp::load_vislists(&entries[VISLIST_ENTRY], &mut bspreader),
            nodes: Bsp::load_nodes(&entries[NODE_ENTRY], &mut bspreader),
            surfaces: Bsp::load_surfaces(&entries[SURFACE_ENTRY], &mut bspreader),
            faces: Bsp::load_faces(&entries[FACE_ENTRY], &mut bspreader),
            lightmaps: Bsp::load_lightmaps(&entries[LIGHTMAP_ENTRY], &mut bspreader),
            clipnodes: Bsp::load_clipnodes(&entries[CLIPNODE_ENTRY], &mut bspreader),
            leaves: Bsp::load_leaves(&entries[LEAF_ENTRY], &mut bspreader),
            facelist: Bsp::load_facelist(&entries[FACELIST_ENTRY], &mut bspreader),
            models: Bsp::load_models(&entries[MODEL_ENTRY], &mut bspreader),
        };

        result
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
            let surface_id = self.faces[first_face].surface_id;
            let mut face_count = 0;

            while self.faces[first_face + face_count].surface_id == surface_id {
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

            let command_buffer = DrawCommandsIndicesBuffer::empty(display, commands.len()).unwrap();
            command_buffer.write(commands.as_slice());

            let surf = &self.surfaces[surface_id as usize];
            let tex = &self.textures[surf.tex_id as usize];

            let uniforms = uniform! {
                perspective: *Mat4::perspective(w as f32, h as f32, math::PI / 2.0),
                view: **view_matrix,
                world: *Mat4::identity(),
                s_vector: surf.s_vector,
                s_offset: surf.s_offset,
                tex_width: tex.w as f32,
                t_vector: surf.t_vector,
                t_offset: surf.t_offset,
                tex_height: tex.h as f32,
                tex: tex.tex.sampled()
                            .magnify_filter(MagnifySamplerFilter::Nearest)
                            .minify_filter(MinifySamplerFilter::LinearMipmapLinear)
                            .wrap_function(SamplerWrapFunction::Repeat),
            };

            let indices = command_buffer.with_index_buffer(&self.indices);
            target.draw(
                &self.vertices,
                indices,
                &program,
                &uniforms,
                &gfx::get_draw_parameters()).unwrap();

            first_face += face_count;
        }

        target.finish().unwrap();
    }

    fn find_leaf<V>(&self, point: V) -> &Leaf where V: AsRef<Vec3> {
        let mut node_index = 0;

        while node_index & (1 << 15) == 0 {
            let node = &self.nodes[node_index];
            let plane = &self.planes[node.plane_id as usize];

            if point.as_ref().dot(&plane.normal) - plane.offset < 0.0 {
                node_index = node.front as usize;
            } else {
                node_index = node.back as usize;
            }
        }

        &self.leaves[node_index]
    }

    /// Decompress the visibility list for a given leaf and return a list of references to the
    /// leaves that are visible from it.
    // TODO: return an Option<Vec<&Leaf>>?
    fn get_visible_leaves(&self, leaf: &Leaf) -> Vec<&Leaf> {
        match leaf.vislist_id {
            -1 => self.leaves.iter().collect(),
            v => {
                let mut to_draw = Vec::with_capacity(self.leaves.len());
                let vislist = &self.vislists[v as usize ..];

                let mut byte = 0;
                loop {
                    match vislist[byte] {
                        0 => {
                            // if this byte is 0, then the next byte is the number of bytes to skip
                            byte += 1;
                            byte += vislist[byte] as usize;
                        },
                        x => for shift in (0..8).rev() {
                            if x >> shift & 1 == 1 {
                                to_draw.push(&self.leaves[byte * 8 + x as usize])
                            }
                        }
                    }
                }
                to_draw.shrink_to_fit();
                to_draw
            }
        }
    }
}
