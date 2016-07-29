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
// - Inline parse_edict()?
// - Create project-wide Wad and WadEntry types
// - Replace index fields with direct references where possible

//! The binary space partitioning (BSP) tree is the central data structure in Quake maps.
//!
//! # Overview
//! The primary purpose of the BSP tree is to describe a hierarchy between the geometric facets
//! of a level. Each of the tree's nodes store a hyperplane in point-normal form, which allows
//! the leaf containing a desired point to be located in log(n) time.
//!
//! # Entities
//! The entity dictionary (*edict*) stores information about dynamic functionality in the level,
//! such as spawn points, dynamic lighting and moving geometry.
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
//! For each leaf *l* in the BSP tree, there exists a visibility list (*vislist*) *v* that
//! describes which other leaves are visible from *l*. The vislists are stored as partially
//! run-length encoded bit vectors. For each byte in the vislist:
//!
//! - If the byte is nonzero (i.e. one or more bits set), it is interpreted as-is.
//! - If the byte is zero, then the byte following it is interpreted as a count of zeroed bytes.
//!
//! # Nodes
//! The internal nodes of the tree are responsible for maintaining the hierarchy between
//! hyperplanes, containing the next level down in front and back of each plane.

use std;
use std::collections::HashMap;
use std::convert::From;
use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::process::exit;

use byteorder::{LittleEndian, ReadBytesExt};
use engine;
use gfx::Vertex;
use glium::{IndexBuffer, Texture2d, VertexBuffer};
use glium::index::PrimitiveType;
use glium::backend::glutin_backend::GlutinFacade as Display;
use math::Vec3;
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
    edge_id: u32,
    edge_count: u16,
    surface_id: u16,
    light_kind: FaceLightKind,
    base_light: u8,
    misc_light: [u8; 2],
    lightmap_off: i32,
}

impl fmt::Debug for Face {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Plane #: {} | \
                  Dir'n: {:?} | \
                  Edge index: {} | \
                  Edge count: {:?} | \
                  Surface ID: {} | \
                  Light kind: {:?} | \
                  Light level: {:?} | \
                  Light info: {}, {} | \
                  Lightmap offset: {:?}",
                 self.plane_id,
                 self.side,
                 self.edge_id,
                 self.edge_count,
                 self.surface_id,
                 self.light_kind,
                 self.base_light,
                 self.misc_light[0], self.misc_light[1],
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
struct LeafNode {
    leaftype: LeafType,
    vislist_id: i32,
    bounds: BoundsShort,
    facelist_id: u16,
    face_count: u16,
    sound: LeafSound,
}

impl fmt::Debug for LeafNode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Type: {:?} | Vislist index: {} | Bounds: {:?} | Facelist index: {} | \
                   Face count: {} | Sound: {:?}", self.leaftype, self.vislist_id, self.bounds,
                    self.facelist_id, self.face_count, self.sound)
    }
}

enum Node {
    Internal(InternalNode),
    Leaf(LeafNode),
}

/// A rough approximation of a BSP node used for preliminary collision detection.
struct ClipNode {
    plane_id: u32,
    front: i16,
    back: i16,
}

/// A pair of vertices.
struct Edge {
    start: u16,
    end: u16,
}

impl Edge {
    fn reverse(&self) -> Edge {
        Edge {
            start: self.end,
            end: self.start,
        }
    }
}

impl fmt::Debug for Edge {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} -> {}", self.start, self.end)
    }
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
    vislists: Vec<u8>,
    nodes: Vec<InternalNode>,
    surfaces: Vec<Surface>,
    faces: Vec<Face>,
    lightmaps: Vec<u8>,
    clipnodes: Vec<ClipNode>,
    leaves: Vec<LeafNode>,
    facelist: Vec<u16>,
    edges: Vec<Edge>,
    edgelist: Vec<i32>,
    models: Vec<Model>,
}

fn parse_edict(entstring: &str) -> Option<Vec<HashMap<String, String>>> {
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
            let val = groups.at(2).unwrap().to_string();

            debug!("\tInserting {{ \"{}\" : \"{}\" }}", key, val);
            entity.insert(key, val);
        }
    }

    entities.shrink_to_fit();
    Some(entities)
}

impl Bsp {
    fn load_entities<R>(entry: &Entry, bspreader: &mut BufReader<&mut R>) -> Vec<HashMap<String, String>> where R: Read + Seek {
        bspreader.seek(SeekFrom::Start(entry.offset as u64)).unwrap();
        let entstring: String = {
            let mut _entstring: Vec<u8> = Vec::with_capacity(MAX_ENTSTRING);
            bspreader.read_until(0x00, &mut _entstring).unwrap();
            String::from_utf8(_entstring).unwrap()
        };

        assert!(bspreader.seek(SeekFrom::Current(0)).unwrap() == bspreader.seek(SeekFrom::Start((entry.offset + entry.size) as u64)).unwrap());

        match parse_edict(&entstring) {
            None => {
                error!("Couldn't parse entity dictionary.");
                exit(1);
            }
            Some(e) => e,
        }
    }

    fn load_planes<R>(entry: &Entry, bspreader: &mut BufReader<&mut R>) -> Vec<Plane> where R: Read + Seek {
        bspreader.seek(SeekFrom::Start(entry.offset as u64)).unwrap();
        assert!(entry.size % PLANE_SIZE == 0);
        let plane_count = entry.size / PLANE_SIZE;
        let mut _planes: Vec<Plane> = Vec::with_capacity(plane_count);
        for _ in 0..plane_count {
            let normal = Vec3::new([bspreader.read_f32::<LittleEndian>().unwrap(),
                                    bspreader.read_f32::<LittleEndian>().unwrap(),
                                    bspreader.read_f32::<LittleEndian>().unwrap()]);
            let offset = bspreader.read_f32::<LittleEndian>().unwrap();
            let kind = bspreader.read_i32::<LittleEndian>().unwrap();
            _planes.push(Plane {
                normal: normal,
                offset: offset,
                kind: match kind {
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
        let tex_count = match bspreader.read_i32::<LittleEndian>().unwrap() {
            t if t <= 0 => panic!("Invalid texture count {}", t),
            t => t as usize
        };

        let tex_offsets = {
            let mut _tex_offsets: Vec<usize> = Vec::with_capacity(tex_count);

            for _ in 0..tex_count {
                _tex_offsets.push(match bspreader.read_i32::<LittleEndian>().unwrap() {
                    -1 => continue,
                    t if t < 0 => panic!("Invalid texture count {}", t),
                    t => t as usize
                });
            }
            _tex_offsets
        };

        let mut textures: Vec<Texture> = Vec::with_capacity(tex_count);

        for off in tex_offsets {
            bspreader.seek(SeekFrom::Start((entry.offset + off) as u64)).unwrap();
            let texname = {
                let mut bytes = [0u8; TEX_NAME_MAX];
                bspreader.read(&mut bytes).unwrap();
                let len = {
                    let mut _len = 0;
                    for (pos, i) in (0..TEX_NAME_MAX).enumerate() {
                        if bytes[i] == 0x00 {
                            _len = pos;
                            break;
                        }
                    }
                    _len
                };
                assert!(len != 0);
                let _texname = String::from_utf8(bytes[..len].to_vec()).unwrap();
                _texname
            };

            debug!("Loading \"{}\"", texname);

            let texwidth = bspreader.read_u32::<LittleEndian>().unwrap() as usize;
            assert!(texwidth % 8 == 0);

            let texheight = bspreader.read_u32::<LittleEndian>().unwrap() as usize;
            assert!(texwidth % 8 == 0);

            let texoff = bspreader.read_u32::<LittleEndian>().unwrap();

            // discard other mipmap offsets, we'll let the GPU generate the mipmaps
            for _ in 0..3 {
                bspreader.read_u32::<LittleEndian>().unwrap();
            }

            bspreader.seek(SeekFrom::Start(texoff as u64)).unwrap();
            let mut indices = Vec::with_capacity(texwidth * texheight);
            bspreader.take((texwidth * texheight) as u64).read_to_end(&mut indices).unwrap();
            let tex = engine::tex_from_indexed(display, &indices, texwidth as u32, texheight as u32);
            textures.push(Texture {
                name: texname,
                tex: tex,
            })
        }

        debug!("=== Texture loading complete. ===");
        textures
    }

    fn load_vertices<R>(entry: &Entry, bspreader: &mut BufReader<&mut R>) -> Vec<Vertex> where R: Read + Seek {
        bspreader.seek(SeekFrom::Start(entry.offset as u64)).unwrap();
        assert!(entry.size % (std::mem::size_of::<f32>() * 3) == 0);
        let vertex_count = entry.size / 12;

        let mut vertices = Vec::with_capacity(vertex_count);
        for _ in 0..vertex_count {
            vertices.push(Vertex {
                pos: [bspreader.read_f32::<LittleEndian>().unwrap(),
                      bspreader.read_f32::<LittleEndian>().unwrap(),
                      bspreader.read_f32::<LittleEndian>().unwrap()],
            });
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
                plane_id: bspreader.read_i32::<LittleEndian>().unwrap(),
                front: bspreader.read_u16::<LittleEndian>().unwrap(),
                back: bspreader.read_u16::<LittleEndian>().unwrap(),
                bounds: BoundsShort {
                    min: [bspreader.read_i16::<LittleEndian>().unwrap(),
                          bspreader.read_i16::<LittleEndian>().unwrap(),
                          bspreader.read_i16::<LittleEndian>().unwrap()],
                    max: [bspreader.read_i16::<LittleEndian>().unwrap(),
                          bspreader.read_i16::<LittleEndian>().unwrap(),
                          bspreader.read_i16::<LittleEndian>().unwrap()],
                },
                face_id: bspreader.read_u16::<LittleEndian>().unwrap(),
                face_count: bspreader.read_u16::<LittleEndian>().unwrap(),
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
                s_vector: [bspreader.read_f32::<LittleEndian>().unwrap(),
                           bspreader.read_f32::<LittleEndian>().unwrap(),
                           bspreader.read_f32::<LittleEndian>().unwrap()],
                s_offset: bspreader.read_f32::<LittleEndian>().unwrap(),
                t_vector: [bspreader.read_f32::<LittleEndian>().unwrap(),
                           bspreader.read_f32::<LittleEndian>().unwrap(),
                           bspreader.read_f32::<LittleEndian>().unwrap()],
                t_offset: bspreader.read_f32::<LittleEndian>().unwrap(),
                tex_id: bspreader.read_u32::<LittleEndian>().unwrap(),
                animated: match bspreader.read_u32::<LittleEndian>().unwrap() {
                    0 => false,
                    _ => true,
                },
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
            let face = Face {
                plane_id: bspreader.read_u16::<LittleEndian>().unwrap(),
                side: match bspreader.read_u16::<LittleEndian>().unwrap() {
                    0 => FaceSide::Front,
                    _ => FaceSide::Back,
                },
                edge_id: match bspreader.read_i32::<LittleEndian>().unwrap() {
                    e if e < 0 => panic!("Edge index below zero. (Face at index {}, offset 0x{:X})", i, bspreader.seek(SeekFrom::Current(0)).unwrap()),
                    e => e as u32,
                },
                edge_count: bspreader.read_u16::<LittleEndian>().unwrap(),
                surface_id: bspreader.read_u16::<LittleEndian>().unwrap(),
                light_kind: {
                    match bspreader.read_u8().unwrap() {
                        0 => FaceLightKind::Normal,
                        1 => FaceLightKind::FastPulse,
                        2 => FaceLightKind::SlowPulse,
                        l @ 3...64 => FaceLightKind::Custom(l),
                        255 => FaceLightKind::Disabled,
                        _ => FaceLightKind::Disabled,
                    }
                },
                base_light: bspreader.read_u8().unwrap(),
                misc_light: [bspreader.read_u8().unwrap(), bspreader.read_u8().unwrap()],
                lightmap_off: bspreader.read_i32::<LittleEndian>().unwrap(),
            };
            debug!("Face {}: {:?}", i, face);
            faces.push(face);
        }
        assert!(bspreader.seek(SeekFrom::Current(0)).unwrap() == bspreader.seek(SeekFrom::Start((entry.offset + entry.size) as u64)).unwrap());
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
                plane_id: bspreader.read_u32::<LittleEndian>().unwrap(),
                front: bspreader.read_i16::<LittleEndian>().unwrap(),
                back: bspreader.read_i16::<LittleEndian>().unwrap(),
            });
        }
        assert!(bspreader.seek(SeekFrom::Current(0)).unwrap() == bspreader.seek(SeekFrom::Start((entry.offset + entry.size) as u64)).unwrap());
        clipnodes
    }

    fn load_leaves<R>(entry: &Entry, bspreader: &mut BufReader<&mut R>) -> Vec<LeafNode> where R: Read + Seek {
        bspreader.seek(SeekFrom::Start(entry.offset as u64)).unwrap();
        assert!(entry.size % LEAF_SIZE == 0);

        let leaf_count = entry.size / LEAF_SIZE;
        assert!(leaf_count < MAX_LEAVES);

        let mut leaves = Vec::with_capacity(leaf_count);

        // Leaf 0 represents all space outside the level geometry and is not drawn
        leaves.push(LeafNode {
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
            let leaf = LeafNode {
                leaftype: LeafType::from(bspreader.read_i32::<LittleEndian>().unwrap()),
                vislist_id: bspreader.read_i32::<LittleEndian>().unwrap(),
                bounds: BoundsShort {
                    min: [bspreader.read_i16::<LittleEndian>().unwrap(),
                          bspreader.read_i16::<LittleEndian>().unwrap(),
                          bspreader.read_i16::<LittleEndian>().unwrap()],
                    max: [bspreader.read_i16::<LittleEndian>().unwrap(),
                          bspreader.read_i16::<LittleEndian>().unwrap(),
                          bspreader.read_i16::<LittleEndian>().unwrap()],
                },
                facelist_id: bspreader.read_u16::<LittleEndian>().unwrap(),
                face_count: bspreader.read_u16::<LittleEndian>().unwrap(),
                sound: LeafSound {
                    water: bspreader.read_u8().unwrap(),
                    sky: bspreader.read_u8().unwrap(),
                    acid: bspreader.read_u8().unwrap(),
                    lava: bspreader.read_u8().unwrap(),
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
            facelist.push(bspreader.read_u16::<LittleEndian>().unwrap());
        }
        assert!(bspreader.seek(SeekFrom::Current(0)).unwrap() == bspreader.seek(SeekFrom::Start((entry.offset + entry.size) as u64)).unwrap());
        facelist
    }

    fn load_edges<R>(entry: &Entry, bspreader: &mut BufReader<&mut R>) -> Vec<Edge> where R: Read + Seek {
        bspreader.seek(SeekFrom::Start(entry.offset as u64)).unwrap();
        assert!(entry.size % EDGE_SIZE == 0);
        let edge_count = entry.size / EDGE_SIZE;
        let mut edges = Vec::with_capacity(edge_count);
        for _ in 0..edge_count {
            edges.push(Edge {
                start: bspreader.read_u16::<LittleEndian>().unwrap(),
                end: bspreader.read_u16::<LittleEndian>().unwrap(),
            });
        }

        assert!(edges[0].start == 0 && edges[0].end == 0);

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
            let edge_entry = bspreader.read_i32::<LittleEndian>().unwrap();
            debug!("Edge table {}: {}", i + 1, edge_entry);
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
                    min: [bspreader.read_f32::<LittleEndian>().unwrap(),
                          bspreader.read_f32::<LittleEndian>().unwrap(),
                          bspreader.read_f32::<LittleEndian>().unwrap()],
                    max: [bspreader.read_f32::<LittleEndian>().unwrap(),
                          bspreader.read_f32::<LittleEndian>().unwrap(),
                          bspreader.read_f32::<LittleEndian>().unwrap()],
                },
                origin: [bspreader.read_f32::<LittleEndian>().unwrap(),
                         bspreader.read_f32::<LittleEndian>().unwrap(),
                         bspreader.read_f32::<LittleEndian>().unwrap()],
                node_ids: [bspreader.read_i32::<LittleEndian>().unwrap(),
                           bspreader.read_i32::<LittleEndian>().unwrap(),
                           bspreader.read_i32::<LittleEndian>().unwrap(),
                           bspreader.read_i32::<LittleEndian>().unwrap()],
                leaf_count: bspreader.read_i32::<LittleEndian>().unwrap(),
                face_id: bspreader.read_i32::<LittleEndian>().unwrap(),
                face_count: bspreader.read_i32::<LittleEndian>().unwrap(),
            });
        }
        assert!(bspreader.seek(SeekFrom::Current(0)).unwrap() == bspreader.seek(SeekFrom::Start((entry.offset + entry.size) as u64)).unwrap());
        models
    }

    pub fn load<R>(display: &Display, bspfile: &mut R) -> Bsp where R: Read + Seek {
        let mut bspreader = BufReader::new(bspfile);
        let version = bspreader.read_i32::<LittleEndian>().unwrap();
        assert_eq!(version, VERSION);

        let entries: Vec<Entry> = {
            let mut _entries = Vec::with_capacity(15);
            for _ in 0..15 {
                _entries.push(Entry {
                    offset: match bspreader.read_i32::<LittleEndian>().unwrap() {
                        o if o < 0 => panic!("Invalid offset ({})", o),
                        o => o as usize,
                    },

                    size: match bspreader.read_i32::<LittleEndian>().unwrap() {
                        s if s < 0 => panic!("Invalid size value {}", s),
                        s => s as usize,
                    },
                })
            }
            _entries
        };



        let result = Bsp {
            entities: Bsp::load_entities(&entries[ENTITY_ENTRY], &mut bspreader),
            planes: Bsp::load_planes(&entries[PLANE_ENTRY], &mut bspreader),
            textures: Bsp::load_textures(&display, &entries[MIPTEX_ENTRY], &mut bspreader),
            vertices: VertexBuffer::new(display, &Bsp::load_vertices(&entries[VERTEX_ENTRY], &mut bspreader)).unwrap(),
            vislists: Bsp::load_vislists(&entries[VISLIST_ENTRY], &mut bspreader),
            nodes: Bsp::load_nodes(&entries[NODE_ENTRY], &mut bspreader),
            surfaces: Bsp::load_surfaces(&entries[SURFACE_ENTRY], &mut bspreader),
            faces: Bsp::load_faces(&entries[FACE_ENTRY], &mut bspreader),
            lightmaps: Bsp::load_lightmaps(&entries[LIGHTMAP_ENTRY], &mut bspreader),
            clipnodes: Bsp::load_clipnodes(&entries[CLIPNODE_ENTRY], &mut bspreader),
            leaves: Bsp::load_leaves(&entries[LEAF_ENTRY], &mut bspreader),
            facelist: Bsp::load_facelist(&entries[FACELIST_ENTRY], &mut bspreader),
            edges: Bsp::load_edges(&entries[EDGE_ENTRY], &mut bspreader),
            edgelist: Bsp::load_edgelist(&entries[EDGELIST_ENTRY], &mut bspreader),
            models: Bsp::load_models(&entries[MODEL_ENTRY], &mut bspreader),
        };

        for (i, face) in result.faces.iter().enumerate() {
            debug!("Face {}:", i);
            result.gen_face_indices(display, face);
        }
        result
    }

    fn gen_face_indices(&self, display: &Display, face: &Face) -> IndexBuffer<u32> {
        for i in face.edge_id..face.edge_id + face.edge_count as u32 {
            match self.edgelist[i as usize] {
                x if x < 0 => debug!("Edge {}: {:?}", i, self.edges[-x as usize].reverse()),
                x if x > 0 => debug!("Edge {}: {:?}", i, self.edges[x as usize]),
                _ => (),
            }
        }
        IndexBuffer::empty(display, PrimitiveType::TrianglesList, 10).unwrap()
    }

    fn find_leaf<V>(&self, point: V) -> &LeafNode where V: AsRef<Vec3> {
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
    // TODO: return an Option<Vec<&LeafNode>>?
    fn get_visible_leaves(&self, leaf: &LeafNode) -> Vec<&LeafNode> {
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
