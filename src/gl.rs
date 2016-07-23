use std::fs::File;
use std::io::Read;
use std::path::Path;

use engine;
use mdl;

use byteorder::{LittleEndian, ReadBytesExt};
use glium;
use glium::Texture2d;
use glium::backend::glutin_backend::GlutinFacade as Display;

#[derive(Copy, Clone)]
pub struct Vertex {
    pub pos: [f32; 3],
}
implement_vertex!(Vertex, pos);

#[derive(Copy, Clone)]
pub struct TexCoord {
    pub texcoord: [f32; 2],
}
implement_vertex!(TexCoord, texcoord);

pub struct GlMdlSkinSingle {
    pub texture: Texture2d,
}

impl GlMdlSkinSingle {
    fn from_mdlskinsingle(display: &Display, src: &mdl::SkinSingle, width: u32, height: u32) -> Option<Self> {
        Some(GlMdlSkinSingle {
            texture: engine::tex_from_indexed(display, &src.indices, width, height),
        })
    }
}

pub struct GlMdlSkinGroup {
    times: Vec<f32>,
    textures: Vec<Texture2d>,
}

impl GlMdlSkinGroup {
    fn from_mdlskingroup(display: &Display, src: &mdl::SkinGroup, width: u32, height: u32) -> Option<Self> {
        // TODO: optimize for one map() call
        Some(GlMdlSkinGroup {
            times: src.skins
                      .iter()
                      .map(|x| x.time)
                      .collect(),
            textures: src.skins
                         .iter()
                         .map(|x| engine::tex_from_indexed(display, &x.indices, width, height))
                         .collect(),
        })
    }
}

pub enum GlMdlSkin {
    Single(GlMdlSkinSingle),
    Group(GlMdlSkinGroup),
}

impl GlMdlSkin {
    fn from_mdlskin(display: &Display, src: &mdl::Skin, width: u32, height: u32) -> Option<GlMdlSkin> {
        Some(match *src {
            mdl::Skin::Single(ref s) => GlMdlSkin::Single(
                    GlMdlSkinSingle::from_mdlskinsingle(
                            display,
                            &s,
                            width,
                            height)
                        .expect("")),

            mdl::Skin::Group(ref g) => GlMdlSkin::Group(
                    GlMdlSkinGroup::from_mdlskingroup(
                            display,
                            &g,
                            width,
                            height)
                        .expect("")),
        })
    }
}

pub struct GlMdlFrameSingle {
    pub min: Vertex,
    pub max: Vertex,
    pub name: String,
    pub vertices: glium::VertexBuffer<Vertex>,
}

impl GlMdlFrameSingle {
    pub fn from_mdlframesingle(display: &Display, src: &mdl::FrameSingle, scale: &[f32; 3]) -> Option<Self> {
        let scale_vertex = | pos: [u8; 3] | Vertex { pos: [pos[0] as f32 * scale[0],
                                                           pos[1] as f32 * scale[1],
                                                           pos[2] as f32 * scale[2]]};

        Some( GlMdlFrameSingle {
            min: scale_vertex(src.min.pos),
            max: scale_vertex(src.max.pos),
            name: src.name.to_owned(),
            vertices: glium::VertexBuffer::new(display, src.data.iter()
                                                               .map(|v| scale_vertex(v.pos))
                                                               .collect::<Vec<Vertex>>()
                                                               .as_slice())
                                                               .expect("Vertex buffer creation failed"),
        })
    }
}

pub struct GlMdlFrameGroup {
    pub min: Vertex,
    pub max: Vertex,
    pub times: Vec<f32>,
    pub frames: Vec<GlMdlFrameSingle>,
}

pub enum GlMdlFrame {
    Single(GlMdlFrameSingle),
    Group(GlMdlFrameGroup),
}

pub struct GlMdl {
    pub skins: Vec<GlMdlSkin>,
    pub texcoords: glium::VertexBuffer<TexCoord>,
    pub indices: glium::IndexBuffer<u32>,
    pub frames: Vec<GlMdlFrame>,
}

impl GlMdl {
    pub fn load<P: AsRef<Path>>(display: &Display, path: P) -> Result<GlMdl, ()> {
        let mut mdl_file = File::open(path).unwrap();

        match mdl_file.read_i32::<LittleEndian>().unwrap() {
            mdl::MAGIC => debug!("Verified MDL magic number"),
            _ => panic!("Bad magic number"),
        }

        match mdl_file.read_i32::<LittleEndian>().unwrap() {
            mdl::VERSION => debug!("Verified MDL version"),
            _ => panic!("Bad version number"),
        }

        let scale = [mdl_file.read_f32::<LittleEndian>().unwrap(),
                     mdl_file.read_f32::<LittleEndian>().unwrap(),
                     mdl_file.read_f32::<LittleEndian>().unwrap()];

        let origin = [mdl_file.read_f32::<LittleEndian>().unwrap(),
                      mdl_file.read_f32::<LittleEndian>().unwrap(),
                      mdl_file.read_f32::<LittleEndian>().unwrap()];

        let radius = mdl_file.read_f32::<LittleEndian>().unwrap();

        // discard eye positions
        for _ in 0..3 {
            mdl_file.read_f32::<LittleEndian>().unwrap();
        }

        let skin_count = mdl_file.read_i32::<LittleEndian>().unwrap();
        let skin_w = mdl_file.read_i32::<LittleEndian>().unwrap();
        let skin_h = mdl_file.read_i32::<LittleEndian>().unwrap();
        let vertex_count = mdl_file.read_i32::<LittleEndian>().unwrap();
        let poly_count = mdl_file.read_i32::<LittleEndian>().unwrap();
        let frame_count = mdl_file.read_i32::<LittleEndian>().unwrap();
        let sync_type = mdl_file.read_i32::<LittleEndian>().unwrap();
        let flags = mdl_file.read_i32::<LittleEndian>().unwrap();
        let size = mdl_file.read_i32::<LittleEndian>().unwrap();

        let mut skins: Vec<GlMdlSkin> = Vec::with_capacity(skin_count as usize);

        for _ in 0..skin_count {
            let skin = match mdl_file.read_i32::<LittleEndian>().unwrap() {
                // Static
                0 => {
                    let mut indexed: Vec<u8> = Vec::with_capacity((skin_w * skin_h) as usize);
                    (&mut mdl_file).take((skin_w * skin_h) as u64).read_to_end(&mut indexed).unwrap();
                    GlMdlSkin::Single(GlMdlSkinSingle {
                        texture: engine::tex_from_indexed(display, &indexed, skin_w as u32, skin_h as u32),
                    })
                }

                // Animated
                1 => {
                    let count = mdl_file.read_i32::<LittleEndian>().unwrap();
                    panic!("UNIMPLEMENTED");
                    // for _ in 0..count {
                    //
                    // }
                    // GlMdlSkin::Group(GlMdlSkinGroup {
                    //     times: Vec::new(),
                    //     textures: Vec::new(),
                    // })
                }

                _ => panic!("Bad skin type"),
            };

            skins.push(skin);
        }

        debug!("loaded skins.");

        // NOTE:
        // For the time being, texture coordinate adjustment for vertices which are
        //   1) on the seam, and
        //   2) part of a rear-facing poly
        // is being ignored. This process is optimized in the MDL format for OpenGL immediate mode
        // and I haven't found an elegant way to implement it for glium yet. This may result in
        // textures that look a little goofy around the edges.

        let mut _texcoords: Vec<TexCoord> = Vec::with_capacity(vertex_count as usize);
        // let mut seams: Vec<bool> = Vec::with_capacity(vertex_count as usize);
        for _ in 0..vertex_count {
            // seams.push(match mdl_file.read_i32::<LittleEndian>().unwrap() {
            //     0 => false,
            //     0x20 => true,
            //     _ => panic!("bad seam value"),
            // });
            mdl_file.read_i32::<LittleEndian>().unwrap();

            _texcoords.push(TexCoord {
                texcoord: [mdl_file.read_i32::<LittleEndian>().unwrap() as f32 / skin_w as f32,
                           mdl_file.read_i32::<LittleEndian>().unwrap() as f32 / skin_h as f32],
            });
        }
        let texcoords = glium::VertexBuffer::new(display, &_texcoords).unwrap();

        debug!("loaded texcoords.");

        // let mut poly_facings: Vec<bool> = Vec::with_capacity(poly_count as usize);
        let mut _indices: Vec<u32> = Vec::with_capacity(3 * poly_count as usize);
        for _ in 0..poly_count {
            // poly_facings.push(match mdl_file.read_i32::<LittleEndian>().unwrap() {
            //     0 => false,
            //     1 => true,
            //     _ => panic!("bad front value"),
            // });
            mdl_file.read_i32::<LittleEndian>().unwrap();

            for _ in 0..3 {
                _indices.push(mdl_file.read_i32::<LittleEndian>().unwrap() as u32);
            }
        }
        let indices = glium::IndexBuffer::new(display, glium::index::PrimitiveType::TrianglesList, &_indices).unwrap();

        debug!("loaded indices.");

        let mut frames: Vec<GlMdlFrame> = Vec::with_capacity(frame_count as usize);
        for _ in 0..frame_count {
            frames.push(match mdl_file.read_i32::<LittleEndian>().unwrap() {
                0 => {
                    let min = Vertex {
                        pos: [mdl_file.read_u8().unwrap() as f32 * scale[0],
                              mdl_file.read_u8().unwrap() as f32 * scale[1],
                              mdl_file.read_u8().unwrap() as f32 * scale[2]],
                    };

                    let max = Vertex {
                        pos: [mdl_file.read_u8().unwrap() as f32 * scale[0],
                              mdl_file.read_u8().unwrap() as f32 * scale[1],
                              mdl_file.read_u8().unwrap() as f32 * scale[2]],
                    };

                    let name = {
                        let mut bytes: [u8; 16] = [0; 16];
                        mdl_file.read(&mut bytes).unwrap();
                        let len = {
                            let mut _len = 0;
                            for i in 0..bytes.len() {
                                if bytes[i] == 0 {
                                    _len = i - 1;
                                    break;
                                }
                            }
                            _len
                        };
                        String::from_utf8(bytes[0..len].to_vec()).unwrap()
                    };

                    let mut _vertices: Vec<Vertex> = Vec::with_capacity(vertex_count as usize);
                    for _ in 0..vertex_count {
                        _vertices.push(Vertex{
                            pos: [mdl_file.read_u8().unwrap() as f32 * scale[0],
                                  mdl_file.read_u8().unwrap() as f32 * scale[1],
                                  mdl_file.read_u8().unwrap() as f32 * scale[2]],
                        })
                    }

                    let vertices = glium::VertexBuffer::new(display, &_vertices).unwrap();
                    GlMdlFrame::Single(GlMdlFrameSingle {
                        min: min,
                        max: max,
                        name: name,
                        vertices: vertices,
                    })
                }

                1 => panic!("UNIMPLEMENTED"),
                _ => panic!("Bad frame kind value"),
            });
        }

        Result::Ok(GlMdl {
            skins: skins,
            texcoords: texcoords,
            indices: indices,
            frames: frames,
        })
    }
}
