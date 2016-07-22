use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader, Error as IoError, Read, Seek, SeekFrom};
use std::process::exit;

use byteorder::{LittleEndian, ReadBytesExt};
use regex::Regex;

const BSP_VERSION: i32 = 29;
const BSP_MAX_ENTSTRING: usize = 65536;

const BSP_PLANE_SIZE: usize = 20;
const BSP_NODE_SIZE: usize = 24;
const BSP_LEAF_SIZE: usize = 28;
const BSP_SURFACE_SIZE: usize = 40;
const BSP_FACE_SIZE: usize = 20;
const BSP_TEX_NAME_MAX: usize = 16;

struct Entry {
    offset: usize,
    size: usize,
}

enum PlaneKind {
    AXIAL_X,
    AXIAL_Y,
    AXIAL_Z,
    NAXIAL_X,
    NAXIAL_Y,
    NAXIAL_Z,
}

struct Plane {
    normal: [f32; 3],
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

struct MipTexture {
    name: String,
    width: usize,
    height: usize,
    full: Vec<u8>,
    half: Vec<u8>,
    quarter: Vec<u8>,
    eighth: Vec<u8>,
}

struct Vertex {
    x: f32,
    y: f32,
    z: f32,
}

impl fmt::Display for Vertex {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{{{:.4}, {:.4}, {:.4}}}", self.x, self.y, self.z)
    }
}

struct Surface {
    s_vector: [f32; 3],
    s_offset: f32,
    t_vector: [f32; 3],
    t_offset: f32,
    tex_id: u32,
    animated: bool,
}

enum FaceSide {
    Front,
    Back,
}

impl fmt::Display for FaceSide {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
               "{}",
               match *self {
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

impl fmt::Display for FaceLightKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
               "{}",
               match *self {
                   FaceLightKind::Normal => "Normal",
                   FaceLightKind::FastPulse => "Fast Pulse",
                   FaceLightKind::SlowPulse => "Slow Pulse",
                   FaceLightKind::Disabled => "Disabled",
                   FaceLightKind::Custom(id) => "Custom",
               })
    }
}

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

impl Face {
    fn print(&self) {
        println!("Plane #{}:\n\
                  ----------\n\
                  Direction: {}\n\
                  Edge list ID: {}\n\
                  Edge list length: {}\n\
                  Surface ID: {}\n\
                  Light kind: {}\n\
                  Base light level: {}\n\
                  Lightmap offset: {}",
                 self.plane_id,
                 self.side,
                 self.edge_id,
                 self.edge_count,
                 self.surface_id,
                 self.light_kind,
                 self.base_light,
                 self.lightmap_off);

    }
}

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
    Solid,
    Water,
    Acid,
    Lava,
    Sky,
}

struct LeafSound {
    water: u8,
    sky: u8,
    acid: u8,
    lava: u8,
}

struct LeafNode {
    leaftype: LeafType,
    vislist_id: i32,
    bounds: BoundsShort,
    facelist_id: u16,
    face_count: u16,
    sound: LeafSound,
}

enum Node {
    Internal(InternalNode),
    Leaf(LeafNode),
}

struct Bsp {
    nothing: i32,
}

fn parse_edict(entstring: &str) -> Option<Vec<HashMap<String, String>>> {
    lazy_static! {
        static ref KEYVAL_REGEX: Regex = Regex::new(r#"^"([a-z]+)"\s+"(.+)"$"#).unwrap();
    }

    let _lines: Vec<&str> = entstring.split('\n').collect();
    let mut lines = _lines.iter();
    let mut entities: Vec<HashMap<String, String>> = Vec::with_capacity(128);

    loop {
        let mut line = match lines.next() {
            None => {
                break;
            }
            Some(l) => *l,
        };

        if line == "\u{0}" {
            break;
        }

        if line != "{" {
            println!("Entities must begin with '{{' (got {:?})", line);
            return None;
        }

        let mut entity: HashMap<String, String> = HashMap::with_capacity(8);
        loop {
            line = match lines.next() {
                None => {
                    break;
                }
                Some(l) => *l,
            };

            if line == "}" {
                entity.shrink_to_fit();
                entities.push(entity);
                break;
            }
            let groups = match KEYVAL_REGEX.captures(line) {
                None => {
                    println!("Invalid line in entity list: {}", line);
                    return None;
                }
                Some(g) => g,
            };
            entity.insert(groups.at(1).unwrap().to_string(),
                          groups.at(2).unwrap().to_string());
        }
    }

    entities.shrink_to_fit();
    Some(entities)
}

fn load_bsp(bspfile: &mut File) -> Bsp {
    let mut bspreader = BufReader::new(bspfile);
    let version = bspreader.read_i32::<LittleEndian>().unwrap();
    assert_eq!(version, BSP_VERSION);

    let entries: Vec<Entry> = {
        let mut _entries = Vec::with_capacity(15);
        for i in 0..15 {
            _entries.push(Entry {
                offset: {
                    let _offset = bspreader.read_i32::<LittleEndian>().unwrap();
                    if _offset < 0 {
                        panic!("Invalid offset value {}", _offset);
                    }
                    _offset as usize
                },

                size: {
                    let _size = bspreader.read_i32::<LittleEndian>().unwrap();
                    if _size < 0 {
                        panic!("Invalid size value {}", _size);
                    }
                    _size as usize
                },
            })
        }
        _entries
    };

    bspreader.seek(SeekFrom::Start(entries[0].offset as u64)).unwrap();
    let entstring: String = {
        let mut _entstring: Vec<u8> = Vec::with_capacity(BSP_MAX_ENTSTRING);
        bspreader.read_until(0x00, &mut _entstring);
        String::from_utf8(_entstring).unwrap()
    };

    let entities = match parse_edict(&entstring) {
        None => {
            println!("Couldn't parse entity dictionary.");
            exit(1);
        }
        Some(e) => e,
    };

    bspreader.seek(SeekFrom::Start(entries[1].offset as u64)).unwrap();
    let planes = {
        assert!(entries[1].size % BSP_PLANE_SIZE == 0);
        let plane_count = entries[1].size / BSP_PLANE_SIZE;
        let mut _planes: Vec<Plane> = Vec::with_capacity(plane_count);
        for _ in 0..plane_count {
            let normal: [f32; 3] = [bspreader.read_f32::<LittleEndian>().unwrap(),
                                    bspreader.read_f32::<LittleEndian>().unwrap(),
                                    bspreader.read_f32::<LittleEndian>().unwrap()];
            let offset = bspreader.read_f32::<LittleEndian>().unwrap();
            let kind = bspreader.read_i32::<LittleEndian>().unwrap();
            _planes.push(Plane {
                normal: normal,
                offset: offset,
                kind: match kind {
                    0 => PlaneKind::AXIAL_X,
                    1 => PlaneKind::AXIAL_Y,
                    2 => PlaneKind::AXIAL_Z,
                    3 => PlaneKind::NAXIAL_X,
                    4 => PlaneKind::NAXIAL_Y,
                    5 => PlaneKind::NAXIAL_Z,
                    _ => panic!("Unrecognized plane kind"),
                },
            });
        }
    };

    bspreader.seek(SeekFrom::Start(entries[2].offset as u64)).unwrap();
    let tex_count = {
        let _tex_count = bspreader.read_i32::<LittleEndian>().unwrap();
        if _tex_count <= 0 {
            panic!("Invalid texture count {}", _tex_count);
        }
        _tex_count as usize
    };

    let tex_offsets = {
        let mut _tex_offsets: Vec<usize> = Vec::with_capacity(tex_count);

        for _ in 0..tex_count {
            let tex_offset = {
                let _tex_offset = bspreader.read_i32::<LittleEndian>().unwrap();
                if _tex_offset < 0 {
                    panic!("Invalid texture count {}", _tex_offset);
                }
                _tex_offset as usize
            };
            _tex_offsets.push(tex_offset);
        }
        _tex_offsets
    };

    let textures = {
        let mut _textures: Vec<MipTexture> = Vec::with_capacity(tex_count);

        for off in tex_offsets {
            bspreader.seek(SeekFrom::Start((entries[2].offset + off) as u64)).unwrap();
            let texname = {
                let mut bytes: [u8; BSP_TEX_NAME_MAX] = [0; BSP_TEX_NAME_MAX];
                bspreader.read(&mut bytes);
                let len = {
                    let mut _len = 0;
                    for (pos, i) in (0..BSP_TEX_NAME_MAX).enumerate() {
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

            println!("Loading \"{}\"", texname);

            let texwidth = bspreader.read_u32::<LittleEndian>().unwrap() as usize;
            assert!(texwidth % 8 == 0);

            let texheight = bspreader.read_u32::<LittleEndian>().unwrap() as usize;
            assert!(texwidth % 8 == 0);

            let texmip_offs: [u32; 4] = [bspreader.read_u32::<LittleEndian>().unwrap(),
                                         bspreader.read_u32::<LittleEndian>().unwrap(),
                                         bspreader.read_u32::<LittleEndian>().unwrap(),
                                         bspreader.read_u32::<LittleEndian>().unwrap()];

            for i in 0..3 {
                bspreader.seek(SeekFrom::Start(texmip_offs[i] as u64)).unwrap();
                let texmip = {
                    let scale = 2usize.pow(i as u32);
                    let w = texwidth / scale;
                    let h = texwidth / scale;
                    let mut _texmip: Vec<u8> = Vec::with_capacity(texwidth * texheight);
                    let bsp_tmp = &mut bspreader;
                    bsp_tmp.take((texwidth * texheight) as u64).read_to_end(&mut _texmip);
                };
            }
        }

        _textures
    };

    println!("=== Texture loading complete. ===");

    // Vertex array
    bspreader.seek(SeekFrom::Start(entries[3].offset as u64));
    // TODO: phrase 12 as sizeof(float) * 3
    assert!(entries[3].size % 12 == 0);
    let vertex_count = entries[3].size / 12;

    let vertices: Vec<Vertex> = {
        let mut _vertices = Vec::with_capacity(vertex_count);
        for _ in 0..vertex_count {
            _vertices.push(Vertex {
                x: bspreader.read_f32::<LittleEndian>().unwrap(),
                y: bspreader.read_f32::<LittleEndian>().unwrap(),
                z: bspreader.read_f32::<LittleEndian>().unwrap(),
            });
        }
        _vertices
    };

    for v in vertices {
        println!("{}", v);
    }

    // Visibility list RLE bit vector
    bspreader.seek(SeekFrom::Start(entries[4].offset as u64));
    let visilists: Vec<u8> = {
        let mut _visilists = Vec::with_capacity(entries[4].size);
        let mut bsp_tmp = &mut bspreader;
        bsp_tmp.take(entries[4].size as u64).read_to_end(&mut _visilists);
        _visilists
    };

    // Internal BSP Nodes
    bspreader.seek(SeekFrom::Start(entries[5].offset as u64));
    let node_count = entries[5].size / BSP_NODE_SIZE;
    let nodes: Vec<InternalNode> = {
        let mut _nodes = Vec::with_capacity(node_count);
        for _ in 0..node_count {
            _nodes.push(InternalNode {
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
        _nodes
    };

    // Surface data
    bspreader.seek(SeekFrom::Start(entries[6].offset as u64));
    assert!(entries[6].size % BSP_SURFACE_SIZE == 0);
    let surface_count = entries[6].size / BSP_SURFACE_SIZE;
    let surfaces: Vec<Surface> = {
        let mut _surfaces = Vec::with_capacity(surface_count);
        for _ in 0..surface_count {
            _surfaces.push(Surface {
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
        _surfaces
    };
    assert!(surfaces.len() == surface_count);

    bspreader.seek(SeekFrom::Start(entries[6].offset as u64));
    assert!(entries[7].size % BSP_FACE_SIZE == 0);
    let face_count = entries[7].size / BSP_FACE_SIZE;
    let faces: Vec<Face> = {
        let mut _faces = Vec::with_capacity(face_count);
        for _ in 0..face_count {
            _faces.push(Face {
                plane_id: bspreader.read_u16::<LittleEndian>().unwrap(),
                side: match bspreader.read_u16::<LittleEndian>().unwrap() {
                    0 => FaceSide::Front,
                    _ => FaceSide::Back,
                },
                edge_id: {
                    let _edge_id = bspreader.read_i32::<LittleEndian>().unwrap();
                    if _edge_id < 0 {
                        panic!("Edge index below zero.");
                    }
                    _edge_id as u32
                },
                edge_count: bspreader.read_u16::<LittleEndian>().unwrap(),
                surface_id: bspreader.read_u16::<LittleEndian>().unwrap(),
                light_kind: {
                    let _light_kind = bspreader.read_u8().unwrap();
                    match _light_kind {
                        0 => FaceLightKind::Normal,
                        1 => FaceLightKind::FastPulse,
                        2 => FaceLightKind::SlowPulse,
                        3...10 => FaceLightKind::Custom(_light_kind),
                        _ => panic!("Invalid light kind."),
                    }
                },
                base_light: bspreader.read_u8().unwrap(),
                misc_light: [bspreader.read_u8().unwrap(), bspreader.read_u8().unwrap()],
                lightmap_off: bspreader.read_i32::<LittleEndian>().unwrap(),
            });
        }
        _faces
    };

    // placeholder
    Bsp { nothing: 0 }
}

fn main() {
    let mut bspfile = match File::open("e1m1.bsp") {
        Err(why) => {
            println!("{}", why);
            exit(1);
        }
        Ok(f) => f,
    };

    let bsp = load_bsp(&mut bspfile);
}
