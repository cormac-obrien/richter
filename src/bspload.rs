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

use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use bsp;
use load::Load;
use lump::Lump;

// As defined in bspfile.h
const VERSION: i32 = 29;

const MAX_HULLS: usize = 4;
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
const MAX_MARKTEXINFOS: usize = 65535;
const MAX_TEXINFOS: usize = 4096;
const MAX_EDGES: usize = 256000;
const MAX_SURFEDGES: usize = 512000;
const MAX_TEXTURES: usize = 0x200000;
const MAX_LIGHTMAP: usize = 0x100000;
const MAX_VISLIST: usize = 0x100000;

const PLANE_SIZE: usize = 20;
const NODE_SIZE: usize = 24;
const LEAF_SIZE: usize = 28;
const TEXINFO_SIZE: usize = 40;
const FACE_SIZE: usize = 20;
const CLIPNODE_SIZE: usize = 8;
const MARKSURFACE_SIZE: usize = 2;
const EDGE_SIZE: usize = 4;
const SURFEDGE_SIZE: usize = 4;
const MODEL_SIZE: usize = 64;
const VERTEX_SIZE: usize = 12;
const TEX_NAME_MAX: usize = 16;

const MIPLEVELS: usize = 4;
const NUM_AMBIENTS: usize = 4;

enum LumpId {
    Entities = 0,
    Planes = 1,
    Textures = 2,
    Vertices = 3,
    Visibility = 4,
    Nodes = 5,
    TextureInfo = 6,
    Faces = 7,
    Lightmaps = 8,
    ClipNodes = 9,
    Leaves = 10,
    MarkSurfaces = 11,
    Edges = 12,
    SurfEdges = 13,
    Models = 14,
}

// dplane_t
#[repr(C)]
pub struct DiskPlane {
    pub normal: [f32; 3],
    pub dist: f32,
    pub kind: i32,
}

// dmiptexlump_t
#[repr(C)]
pub struct DiskTextureLump {
    pub count: i32,
    pub offsets: Box<[i32]>,
}

// miptex_t
#[repr(C)]
pub struct DiskTexture {
    pub name: [u8; 16],
    pub width: u32,
    pub height: u32,
    pub mipmaps: [Box<[u8]>; MIPLEVELS],
}

// dvertex_t
#[repr(C)]
pub struct DiskVertex {
    pub position: [f32; 3],
}

// dnode_t
#[repr(C)]
pub struct DiskNode {
    pub plane_id: i32,
    pub children: [i16; 2],
    pub mins: [i16; 3],
    pub maxs: [i16; 3],
    pub face_id: u16,
    pub face_count: u16,
}

// texinfo_t
#[repr(C)]
pub struct DiskTextureInfo {
    pub vecs: [[f32; 4]; 2],
    pub tex_id: i32,
    pub flags: i32,
}

// dface_t
#[repr(C)]
pub struct DiskFace {
    pub plane_id: i16,
    pub side: i16,
    pub edge_id: i32,
    pub edge_count: i16,
    pub texinfo: i16,
    pub styles: [u8; bsp::MAX_LIGHTSTYLE_COUNT],
    pub light_off: i32,
}

// dclipnode_t
#[repr(C)]
pub struct DiskClipNode {
    pub plane_id: i32,
    pub children: [i16; 2],
}

// dleaf_t
#[repr(C)]
pub struct DiskLeaf {
    pub contents: i32,
    pub vis_offset: i32,
    pub mins: [i16; 3],
    pub maxs: [i16; 3],
    pub marksurf_id: u16,
    pub marksurf_count: u16,
    pub sounds: [u8; NUM_AMBIENTS],
}

// dedge_t
#[repr(C)]
pub struct DiskEdge {
    pub vertex_ids: [u16; 2],
}

// dmodel_t
#[repr(C)]
pub struct DiskModel {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub origin: [f32; 3],
    pub roots: [i32; MAX_HULLS],
    pub leaf_count: i32,
    pub face_id: i32,
    pub face_count: i32,
}

pub struct DiskBsp {
    pub entstring: String,
    pub planes: Box<[DiskPlane]>,
    pub textures: Box<[DiskTexture]>,
    pub vertices: Box<[DiskVertex]>,
    pub visibility: Box<[u8]>,
    pub nodes: Box<[DiskNode]>,
    pub texinfo: Box<[DiskTextureInfo]>,
    pub faces: Box<[DiskFace]>,
    pub lightmaps: Box<[u8]>,
    pub clipnodes: Box<[DiskClipNode]>,
    pub leaves: Box<[DiskLeaf]>,
    pub marksurfaces: Box<[u16]>,
    pub edges: Box<[DiskEdge]>,
    pub surfedges: Box<[i32]>,
    pub models: Box<[DiskModel]>,
}

impl DiskBsp {
    pub fn load<R>(bspfile: &mut R) -> DiskBsp
            where R: Read + Seek {
        let mut bspreader = BufReader::new(bspfile);
        let version = bspreader.load_i32le();
        assert_eq!(version, VERSION);

        let mut lumps = Vec::with_capacity(15);
        for _ in 0..15 {
            lumps.push(Lump {
                offset: bspreader.load_i32le() as usize,
                size: bspreader.load_i32le() as usize,
            });
        }

        // load entity data
        let mut lump = &lumps[LumpId::Entities as usize];
        bspreader.seek(SeekFrom::Start(lump.offset as u64)).unwrap();
        let mut entdata = Vec::with_capacity(MAX_ENTSTRING);
        bspreader.read_until(0x00, &mut entdata).unwrap();
        let entstring = String::from_utf8(entdata).unwrap();
        assert_eq!(bspreader.seek(SeekFrom::Current(0)).unwrap(),
                   bspreader.seek(SeekFrom::Start((lump.offset + lump.size) as u64)).unwrap());

        // load planes
        lump = &lumps[LumpId::Planes as usize];
        bspreader.seek(SeekFrom::Start(lump.offset as u64)).unwrap();
        assert_eq!(lump.size % PLANE_SIZE, 0);
        let plane_count = lump.size / PLANE_SIZE;
        let mut planes = Vec::with_capacity(plane_count);
        for _ in 0..plane_count {
            planes.push(DiskPlane {
                normal: [bspreader.load_f32le(), bspreader.load_f32le(), bspreader.load_f32le()],
                dist: bspreader.load_f32le(),
                kind: bspreader.load_i32le(),
            });
        }
        assert_eq!(bspreader.seek(SeekFrom::Current(0)).unwrap(),
                   bspreader.seek(SeekFrom::Start((lump.offset + lump.size) as u64)).unwrap());

        // load textures
        lump = &lumps[LumpId::Textures as usize];
        bspreader.seek(SeekFrom::Start(lump.offset as u64)).unwrap();
        let tex_count = bspreader.load_i32le() as usize;
        let mut tex_offsets = Vec::with_capacity(tex_count);
        for _ in 0..tex_count {
            tex_offsets.push(bspreader.load_i32le() as usize);
        }
        let mut textures = Vec::with_capacity(tex_count);
        for t in 0..tex_count {
            bspreader.seek(SeekFrom::Start((lump.offset + tex_offsets[t]) as u64)).unwrap();
            let mut tex_name: [u8; 16] = [0; 16];
            bspreader.read(&mut tex_name).unwrap();
            let width = bspreader.load_u32le();
            let height = bspreader.load_u32le();
            let mut mipmap_vec = Vec::new();
            let mut mip_offsets = [0; MIPLEVELS];
            for m in 0..mip_offsets.len() {
                mip_offsets[m] = bspreader.load_u32le() as usize;
            }
            for m in 0..mip_offsets.len() {
                let factor = 2usize.pow(m as u32);
                let mipmap_size = (width as usize / factor) * (height as usize / factor);
                bspreader.seek(SeekFrom::Start((lump.offset + tex_offsets[t] + mip_offsets[m]) as u64)).unwrap();
                let mut mip_data = Vec::with_capacity(mipmap_size);
                (&mut bspreader).take(mipmap_size as u64).read_to_end(&mut mip_data).unwrap();
                mipmap_vec.push(mip_data.into_boxed_slice());
            }
            textures.push(DiskTexture {
                name: tex_name,
                width: width,
                height: height,
                mipmaps: [mipmap_vec[0].clone(), mipmap_vec[1].clone(),
                          mipmap_vec[2].clone(), mipmap_vec[3].clone()],
            });
        }
        assert_eq!(bspreader.seek(SeekFrom::Current(0)).unwrap(),
                   bspreader.seek(SeekFrom::Start((lump.offset + lump.size) as u64)).unwrap());

        // load vertices
        lump = &lumps[LumpId::Vertices as usize];
        bspreader.seek(SeekFrom::Start(lump.offset as u64)).unwrap();
        assert_eq!(lump.size % VERTEX_SIZE, 0);
        let vert_count = lump.size / VERTEX_SIZE;
        let mut vertices = Vec::with_capacity(vert_count);
        for _ in 0..vert_count {
            let mut position = [0.0; 3];
            for i in 0..position.len() {
                position[i] = bspreader.load_f32le();
            }

            vertices.push(DiskVertex {
                position: position,
            });
        }
        assert_eq!(bspreader.seek(SeekFrom::Current(0)).unwrap(),
                   bspreader.seek(SeekFrom::Start((lump.offset + lump.size) as u64)).unwrap());

        // load visibility
        lump = &lumps[LumpId::Visibility as usize];
        bspreader.seek(SeekFrom::Start(lump.offset as u64)).unwrap();
        let mut vislists: Vec<u8> = Vec::with_capacity(lump.size);
        (&mut bspreader).take(lump.size as u64).read_to_end(&mut vislists).unwrap();
        assert_eq!(bspreader.seek(SeekFrom::Current(0)).unwrap(),
                   bspreader.seek(SeekFrom::Start((lump.offset + lump.size) as u64)).unwrap());

        // load nodes
        lump = &lumps[LumpId::Nodes as usize];
        bspreader.seek(SeekFrom::Start(lump.offset as u64)).unwrap();
        assert_eq!(lump.size % NODE_SIZE, 0);
        let node_count = lump.size / NODE_SIZE;
        let mut nodes = Vec::with_capacity(node_count);
        for _ in 0..node_count {
            let plane_id = bspreader.load_i32le();
            let children = [bspreader.load_i16le(), bspreader.load_i16le()];
            let mut mins = [0i16; 3];
            for i in 0..mins.len() {
                mins[i] = bspreader.load_i16le();
            }
            let mut maxs = [0i16; 3];
            for i in 0..maxs.len() {
                maxs[i] = bspreader.load_i16le();
            }
            let face_id = bspreader.load_u16le();
            let face_count = bspreader.load_u16le();

            nodes.push(DiskNode {
                plane_id: plane_id,
                children: children,
                mins: mins,
                maxs: maxs,
                face_id: face_id,
                face_count: face_count,
            });
        }
        assert_eq!(bspreader.seek(SeekFrom::Current(0)).unwrap(),
                   bspreader.seek(SeekFrom::Start((lump.offset + lump.size) as u64)).unwrap());

        // load texinfo
        lump = &lumps[LumpId::TextureInfo as usize];
        bspreader.seek(SeekFrom::Start(lump.offset as u64)).unwrap();
        assert_eq!(lump.size % TEXINFO_SIZE, 0);
        let texinfo_count = lump.size / TEXINFO_SIZE;
        let mut texinfos = Vec::with_capacity(texinfo_count);
        for _ in 0..texinfo_count {
            let mut vecs = [[0.0; 4]; 2];
            for i in 0..vecs.len() {
                for j in 0..vecs[0].len() {
                    vecs[i][j] = bspreader.load_f32le();
                }
            }
            let tex_id = bspreader.load_i32le();
            let flags = bspreader.load_i32le();
            texinfos.push(DiskTextureInfo {
                vecs: vecs,
                tex_id: tex_id,
                flags: flags,
            });
        }
        assert_eq!(bspreader.seek(SeekFrom::Current(0)).unwrap(),
                   bspreader.seek(SeekFrom::Start((lump.offset + lump.size) as u64)).unwrap());

        // load faces
        lump = &lumps[LumpId::Faces as usize];
        bspreader.seek(SeekFrom::Start(lump.offset as u64)).unwrap();
        assert_eq!(lump.size % FACE_SIZE, 0);
        let face_count = lump.size / FACE_SIZE;
        let mut faces = Vec::with_capacity(face_count);
        for _ in 0..face_count {
            let plane_id = bspreader.load_i16le();
            let side = bspreader.load_i16le();
            let edge_id = bspreader.load_i32le();
            let edge_count = bspreader.load_i16le();
            assert!(edge_count >= 3);
            let texinfo = bspreader.load_i16le();
            let mut styles = [0; bsp::MAX_LIGHTSTYLE_COUNT];
            for i in 0..styles.len() {
                styles[i] = bspreader.load_u8();
            }
            let light_off = bspreader.load_i32le();
            faces.push(DiskFace {
                plane_id: plane_id,
                side: side,
                edge_id: edge_id,
                edge_count: edge_count,
                texinfo: texinfo,
                styles: styles,
                light_off: light_off,
            });
        }
        assert_eq!(bspreader.seek(SeekFrom::Current(0)).unwrap(),
                   bspreader.seek(SeekFrom::Start((lump.offset + lump.size) as u64)).unwrap());

        // load lightmaps
        lump = &lumps[LumpId::Lightmaps as usize];
        bspreader.seek(SeekFrom::Start(lump.offset as u64)).unwrap();
        let mut lightmaps = Vec::with_capacity(lump.size);
        (&mut bspreader).take(lump.size as u64).read_to_end(&mut lightmaps).unwrap();
        assert_eq!(bspreader.seek(SeekFrom::Current(0)).unwrap(),
                   bspreader.seek(SeekFrom::Start((lump.offset + lump.size) as u64)).unwrap());

        // load clipnodes
        lump = &lumps[LumpId::ClipNodes as usize];
        bspreader.seek(SeekFrom::Start(lump.offset as u64)).unwrap();
        assert_eq!(lump.size % CLIPNODE_SIZE, 0);
        let clipnode_count = lump.size / CLIPNODE_SIZE;
        let mut clipnodes = Vec::with_capacity(clipnode_count);
        for _ in 0..clipnode_count {
            clipnodes.push(DiskClipNode {
                plane_id: bspreader.load_i32le(),
                children: [bspreader.load_i16le(),
                           bspreader.load_i16le()],
            });
        }
        assert_eq!(bspreader.seek(SeekFrom::Current(0)).unwrap(),
                   bspreader.seek(SeekFrom::Start((lump.offset + lump.size) as u64)).unwrap());

        // load leaves
        lump = &lumps[LumpId::Leaves as usize];
        bspreader.seek(SeekFrom::Start(lump.offset as u64)).unwrap();
        assert_eq!(lump.size % LEAF_SIZE, 0);
        let leaf_count = lump.size / LEAF_SIZE;
        let mut leaves = Vec::with_capacity(leaf_count);
        for _ in 0..leaf_count {
            let contents = bspreader.load_i32le();
            let vis_offset = bspreader.load_i32le();
            let mut mins = [0i16; 3];
            for i in 0..mins.len() {
                mins[i] = bspreader.load_i16le();
            }
            let mut maxs = [0i16; 3];
            for i in 0..maxs.len() {
                maxs[i] = bspreader.load_i16le();
            }
            let marksurf_id = bspreader.load_u16le();
            let marksurf_count = bspreader.load_u16le();
            let mut sounds = [0u8; NUM_AMBIENTS];
            bspreader.read(&mut sounds).unwrap();
            leaves.push(DiskLeaf {
                contents: contents,
                vis_offset: vis_offset,
                mins: mins,
                maxs: maxs,
                marksurf_id: marksurf_id,
                marksurf_count: marksurf_count,
                sounds: sounds,
            });
        }
        assert_eq!(bspreader.seek(SeekFrom::Current(0)).unwrap(),
                   bspreader.seek(SeekFrom::Start((lump.offset + lump.size) as u64)).unwrap());

        // load mark surfaces
        lump = &lumps[LumpId::MarkSurfaces as usize];
        bspreader.seek(SeekFrom::Start(lump.offset as u64)).unwrap();
        assert_eq!(lump.size % MARKSURFACE_SIZE, 0);
        let marksurface_count = lump.size / MARKSURFACE_SIZE;
        let mut marksurfaces = Vec::with_capacity(marksurface_count);
        for _ in 0..marksurface_count {
            marksurfaces.push(bspreader.load_u16le());
        }
        assert_eq!(bspreader.seek(SeekFrom::Current(0)).unwrap(),
                   bspreader.seek(SeekFrom::Start((lump.offset + lump.size) as u64)).unwrap());

        // load edges
        lump = &lumps[LumpId::Edges as usize];
        bspreader.seek(SeekFrom::Start(lump.offset as u64)).unwrap();
        assert_eq!(lump.size % EDGE_SIZE, 0);
        let edge_count = lump.size / EDGE_SIZE;
        let mut edges = Vec::with_capacity(edge_count);
        for _ in 0..edge_count {
            edges.push(DiskEdge {
                vertex_ids: [bspreader.load_u16le(), bspreader.load_u16le()],
            });
        }
        assert_eq!(bspreader.seek(SeekFrom::Current(0)).unwrap(),
                   bspreader.seek(SeekFrom::Start((lump.offset + lump.size) as u64)).unwrap());

        // load surfedges
        lump = &lumps[LumpId::SurfEdges as usize];
        bspreader.seek(SeekFrom::Start(lump.offset as u64)).unwrap();
        assert!(lump.size % SURFEDGE_SIZE == 0);
        let surfedge_count = lump.size / SURFEDGE_SIZE;
        let mut surfedges = Vec::with_capacity(surfedge_count);
        for i in 0..surfedge_count {
            let edge = bspreader.load_i32le();
            debug!("Edge table {}: {}", i, edge);
            surfedges.push(edge);
        }
        assert_eq!(bspreader.seek(SeekFrom::Current(0)).unwrap(),
                   bspreader.seek(SeekFrom::Start((lump.offset + lump.size) as u64)).unwrap());

        // load models
        lump = &lumps[LumpId::Models as usize];
        bspreader.seek(SeekFrom::Start(lump.offset as u64)).unwrap();
        assert!(lump.size % MODEL_SIZE == 0);
        let model_count = lump.size / MODEL_SIZE;
        let mut models = Vec::with_capacity(model_count);
        for _ in 0..model_count {
            let mut mins = [0.0; 3];
            for i in 0..mins.len() {
                mins[i] = bspreader.load_f32le();
            }
            let mut maxs = [0.0; 3];
            for i in 0..maxs.len() {
                maxs[i] = bspreader.load_f32le();
            }
            let mut origin = [0.0; 3];
            for i in 0..origin.len() {
                origin[i] = bspreader.load_f32le();
            }
            let mut roots = [0; MAX_HULLS];
            for i in 0..roots.len() {
                roots[i] = bspreader.load_i32le();
            }
            let leaf_count = bspreader.load_i32le();
            let face_id = bspreader.load_i32le();
            let face_count = bspreader.load_i32le();
            models.push(DiskModel {
                mins: mins,
                maxs: maxs,
                origin: origin,
                roots: roots,
                leaf_count: leaf_count,
                face_id: face_id,
                face_count: face_count,
            });
        }
        assert_eq!(bspreader.seek(SeekFrom::Current(0)).unwrap(),
                   bspreader.seek(SeekFrom::Start((lump.offset + lump.size) as u64)).unwrap());

        DiskBsp {
            entstring: entstring,
            planes: planes.into_boxed_slice(),
            textures: textures.into_boxed_slice(),
            vertices: vertices.into_boxed_slice(),
            visibility: vislists.into_boxed_slice(),
            nodes: nodes.into_boxed_slice(),
            texinfo: texinfos.into_boxed_slice(),
            faces: faces.into_boxed_slice(),
            lightmaps: lightmaps.into_boxed_slice(),
            clipnodes: clipnodes.into_boxed_slice(),
            leaves: leaves.into_boxed_slice(),
            marksurfaces: marksurfaces.into_boxed_slice(),
            edges: edges.into_boxed_slice(),
            surfedges: surfedges.into_boxed_slice(),
            models: models.into_boxed_slice(),
        }
    }
}
