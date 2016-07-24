use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use engine;
use mdl;

use byteorder::{LittleEndian, ReadBytesExt};
use glium;
use glium::Texture2d;
use glium::backend::glutin_backend::GlutinFacade as Display;

pub const MAGIC: i32 = 0x4F504449;
pub const VERSION: i32 = 6;

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

pub struct SkinSingle {
    pub texture: Texture2d,
}

pub struct SkinGroup {
    times: Vec<f32>,
    textures: Vec<Texture2d>,
}

pub enum Skin {
    Single(SkinSingle),
    Group(SkinGroup),
}

pub struct FrameSingle {
    pub min: Vertex,
    pub max: Vertex,
    pub name: String,
    pub vertices: glium::VertexBuffer<Vertex>,
}

pub struct FrameGroup {
    pub min: Vertex,
    pub max: Vertex,
    pub times: Vec<f32>,
    pub frames: Vec<FrameSingle>,
}

pub enum Frame {
    Single(FrameSingle),
    Group(FrameGroup),
}

pub struct Mdl {
    pub origin: [f32; 3],
    pub radius: f32,
    pub skins: Vec<Skin>,
    pub texcoords: glium::VertexBuffer<TexCoord>,
    pub indices: glium::IndexBuffer<u32>,
    pub frames: Vec<Frame>,
}

impl Mdl {
    pub fn load<P: AsRef<Path>>(display: &Display, path: P) -> Result<Mdl, ()> {
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

        debug!("origin is {}, {}, {}", origin[0], origin[1], origin[2]);

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

        let mut skins: Vec<Skin> = Vec::with_capacity(skin_count as usize);

        for _ in 0..skin_count {
            let skin = match mdl_file.read_i32::<LittleEndian>().unwrap() {
                // Static
                0 => {
                    let mut indexed: Vec<u8> = Vec::with_capacity((skin_w * skin_h) as usize);
                    (&mut mdl_file).take((skin_w * skin_h) as u64).read_to_end(&mut indexed).unwrap();
                    Skin::Single(SkinSingle {
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

        debug!("Loaded skins. Current position in file is 0x{:X}", mdl_file.seek(SeekFrom::Current(0)).unwrap());

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

        debug!("Loaded texcoords. Current position in file is 0x{:X}", mdl_file.seek(SeekFrom::Current(0)).unwrap());

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

        let mut frames: Vec<Frame> = Vec::with_capacity(frame_count as usize);
        for _ in 0..frame_count {
            frames.push(match mdl_file.read_i32::<LittleEndian>().unwrap() {
                0 => {
                    let min = Vertex {
                        pos: [mdl_file.read_u8().unwrap() as f32 * scale[0] + origin[0],
                              mdl_file.read_u8().unwrap() as f32 * scale[1] + origin[1],
                              mdl_file.read_u8().unwrap() as f32 * scale[2] + origin[2]],
                    };

                    mdl_file.read_u8().unwrap(); // discard vertex normal

                    let max = Vertex {
                        pos: [mdl_file.read_u8().unwrap() as f32 * scale[0] + origin[0],
                              mdl_file.read_u8().unwrap() as f32 * scale[1] + origin[1],
                              mdl_file.read_u8().unwrap() as f32 * scale[2] + origin[2]],
                    };

                    mdl_file.read_u8().unwrap(); // discard vertex normal

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
                        String::from_utf8(bytes[0..(len + 1)].to_vec()).unwrap()
                    };

                    debug!("Frame name: {}", name);

                    let mut _vertices: Vec<Vertex> = Vec::with_capacity(vertex_count as usize);
                    for _ in 0..vertex_count {
                        _vertices.push(Vertex{
                            pos: [mdl_file.read_u8().unwrap() as f32 * scale[0] + origin[0],
                                  mdl_file.read_u8().unwrap() as f32 * scale[1] + origin[1],
                                  mdl_file.read_u8().unwrap() as f32 * scale[2] + origin[2]],
                        });
                        println!("{{{}, {}, {}}}", _vertices.last().unwrap().pos[0], _vertices.last().unwrap().pos[1], _vertices.last().unwrap().pos[2]);
                        mdl_file.read_u8().unwrap(); // discard vertex normal
                    }

                    let vertices = glium::VertexBuffer::new(display, &_vertices).unwrap();
                    Frame::Single(FrameSingle {
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

        assert!(mdl_file.seek(SeekFrom::Current(0)).unwrap() == mdl_file.metadata().unwrap().len());

        Result::Ok(Mdl {
            origin: origin,
            radius: radius,
            skins: skins,
            texcoords: texcoords,
            indices: indices,
            frames: frames,
        })
    }
}
