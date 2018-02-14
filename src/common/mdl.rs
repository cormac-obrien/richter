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

use std::io::BufReader;
use std::io::Cursor;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;

use common::engine;
use common::model::SyncType;

use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use cgmath::Vector2;
use cgmath::Vector3;
use chrono::Duration;
use num::FromPrimitive;

pub const MAGIC: i32 = ('I' as i32) << 0 | ('D' as i32) << 8 | ('P' as i32) << 16 |
    ('O' as i32) << 24;
pub const VERSION: i32 = 6;

const HEADER_SIZE: u64 = 84;

pub struct SkinSingle {
    rgba: Box<[u8]>,
}

pub struct SkinGroup {
    intervals: Box<[Duration]>,
    skins: Box<[SkinSingle]>,
}

pub enum Skin {
    Single(SkinSingle),
    Group(SkinGroup),
}

pub struct FrameSingle {
    pub name: String,
    pub min: Vector3<f32>,
    pub max: Vector3<f32>,
    pub vertices: Box<[Vector3<f32>]>,
}

pub struct FrameGroup {
    pub min: Vector3<f32>,
    pub max: Vector3<f32>,
    pub times: Vec<Duration>,
    pub frames: Vec<FrameSingle>,
}

pub enum Frame {
    Single(FrameSingle),
    Group(FrameGroup),
}

pub struct AliasModel {
    pub origin: Vector3<f32>,
    pub radius: f32,
    pub skins: Box<[Skin]>,
    pub texcoords: Box<[Vector2<f32>]>,
    pub indices: Box<[u32]>,
    pub frames: Box<[Frame]>,
}

pub fn load(data: &[u8]) -> Result<AliasModel, ()> {
    let mut reader = BufReader::new(Cursor::new(data));

    match reader.read_i32::<LittleEndian>().unwrap() {
        MAGIC => debug!("Verified MDL magic number"),
        _ => panic!("Bad magic number"),
    }

    match reader.read_i32::<LittleEndian>().unwrap() {
        VERSION => debug!("Verified MDL version"),
        _ => panic!("Bad version number"),
    }

    let scale = Vector3::new(
        reader.read_f32::<LittleEndian>().unwrap(),
        reader.read_f32::<LittleEndian>().unwrap(),
        reader.read_f32::<LittleEndian>().unwrap(),
    );

    let origin = Vector3::new(
        reader.read_f32::<LittleEndian>().unwrap(),
        reader.read_f32::<LittleEndian>().unwrap(),
        reader.read_f32::<LittleEndian>().unwrap(),
    );

    let radius = reader.read_f32::<LittleEndian>().unwrap();

    let eye_position = Vector3::new(
        reader.read_f32::<LittleEndian>().unwrap(),
        reader.read_f32::<LittleEndian>().unwrap(),
        reader.read_f32::<LittleEndian>().unwrap(),
    );

    let skin_count = reader.read_i32::<LittleEndian>().unwrap();

    let skin_w = match reader.read_i32::<LittleEndian>().unwrap() {
        w if w < 0 => panic!("Negative skin width ({})", w),
        w => w,
    };

    let skin_h = match reader.read_i32::<LittleEndian>().unwrap() {
        h if h < 0 => panic!("Negative skin height ({})", h),
        h => h,
    };

    let vertex_count = match reader.read_i32::<LittleEndian>().unwrap() {
        v if v < 0 => panic!("Negative vertex count ({})", v),
        v => v,
    };

    let poly_count = match reader.read_i32::<LittleEndian>().unwrap() {
        p if p < 0 => panic!("Negative polygon count ({})", p),
        p => p,
    };

    let frame_count = match reader.read_i32::<LittleEndian>().unwrap() {
        f if f < 0 => panic!("Negative frame count ({})", f),
        f => f,
    };

    let sync_type = SyncType::from_i32(reader.read_i32::<LittleEndian>().unwrap());

    let flags = reader.read_i32::<LittleEndian>().unwrap();

    let size = match reader.read_i32::<LittleEndian>().unwrap() {
        s if s < 0 => panic!("Negative size ({})", s),
        s => s,
    };

    if reader.seek(SeekFrom::Current(0)).unwrap() !=
        reader.seek(SeekFrom::Start(HEADER_SIZE)).unwrap()
    {
        panic!("Misaligned read on MDL header");
    }

    let mut skins: Vec<Skin> = Vec::with_capacity(skin_count as usize);

    for _ in 0..skin_count {
        // TODO: add a SkinKind type
        let skin = match reader.read_i32::<LittleEndian>().unwrap() {
            // Static
            0 => {
                let mut indexed: Vec<u8> = Vec::with_capacity((skin_w * skin_h) as usize);
                (&mut reader)
                    .take((skin_w * skin_h) as u64)
                    .read_to_end(&mut indexed)
                    .unwrap();
                Skin::Single(SkinSingle {
                    rgba: engine::indexed_to_rgba(&indexed).into_boxed_slice(),
                })
            }

            // Animated
            1 => {
                // TODO: sanity check this value
                let skin_frame_count = reader.read_i32::<LittleEndian>().unwrap() as usize;

                let mut intervals = Vec::with_capacity(skin_frame_count);
                for _ in 0..skin_frame_count {
                    intervals.push(engine::duration_from_f32(
                        reader.read_f32::<LittleEndian>().unwrap(),
                    ));
                }

                let mut frames = Vec::with_capacity(skin_frame_count);
                for _ in 0..skin_frame_count {
                    let mut indexed: Vec<u8> = Vec::with_capacity((skin_w * skin_h) as usize);
                    (&mut reader)
                        .take((skin_w * skin_h) as u64)
                        .read_to_end(&mut indexed)
                        .unwrap();
                    frames.push(SkinSingle {
                        rgba: engine::indexed_to_rgba(&indexed).into_boxed_slice(),
                    });
                }

                Skin::Group(SkinGroup {
                    intervals: intervals.into_boxed_slice(),
                    skins: frames.into_boxed_slice(),
                })
            }

            _ => panic!("Bad skin type"),
        };

        skins.push(skin);
    }

    // NOTE:
    // For the time being, texture coordinate adjustment for vertices which are
    //   1) on the seam, and
    //   2) part of a rear-facing poly
    // is being ignored. This process is optimized in the MDL format for OpenGL immediate mode
    // and I haven't found an elegant way to implement it for glium yet. This may result in
    // textures that look a little goofy around the edges.

    let mut texcoords: Vec<Vector2<f32>> = Vec::with_capacity(vertex_count as usize);
    // let mut seams: Vec<bool> = Vec::with_capacity(vertex_count as usize);
    for _ in 0..vertex_count {
        // seams.push(match reader.read_i32::<LittleEndian>().unwrap() {
        //     0 => false,
        //     0x20 => true,
        //     _ => panic!("bad seam value"),
        // });
        reader.read_i32::<LittleEndian>().unwrap();

        texcoords.push(Vector2::new(
            reader.read_i32::<LittleEndian>().unwrap() as f32 /
                skin_w as f32,
            reader.read_i32::<LittleEndian>().unwrap() as f32 /
                skin_h as f32,
        ));
    }

    // let mut poly_facings: Vec<bool> = Vec::with_capacity(poly_count as usize);
    let mut indices: Vec<u32> = Vec::with_capacity(3 * poly_count as usize);
    for _ in 0..poly_count {
        // poly_facings.push(match reader.read_i32::<LittleEndian>().unwrap() {
        //     0 => false,
        //     1 => true,
        //     _ => panic!("bad front value"),
        // });
        reader.read_i32::<LittleEndian>().unwrap();

        for _ in 0..3 {
            indices.push(reader.read_i32::<LittleEndian>().unwrap() as u32);
        }
    }

    debug!("loaded indices.");

    let mut frames: Vec<Frame> = Vec::with_capacity(frame_count as usize);
    for _ in 0..frame_count {
        frames.push(match reader.read_i32::<LittleEndian>().unwrap() {
            0 => {
                let min = Vector3::new(
                    reader.read_u8().unwrap() as f32 * scale[0] + origin[0],
                    reader.read_u8().unwrap() as f32 * scale[1] + origin[1],
                    reader.read_u8().unwrap() as f32 * scale[2] + origin[2],
                );

                reader.read_u8().unwrap(); // discard vertex normal

                let max = Vector3::new(
                    reader.read_u8().unwrap() as f32 * scale[0] + origin[0],
                    reader.read_u8().unwrap() as f32 * scale[1] + origin[1],
                    reader.read_u8().unwrap() as f32 * scale[2] + origin[2],
                );

                reader.read_u8().unwrap(); // discard vertex normal

                let name = {
                    let mut bytes: [u8; 16] = [0; 16];
                    reader.read(&mut bytes).unwrap();
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

                let mut vertices: Vec<Vector3<f32>> = Vec::with_capacity(vertex_count as usize);
                for _ in 0..vertex_count {
                    vertices.push(Vector3::new(
                        reader.read_u8().unwrap() as f32 * scale[0] + origin[0],
                        reader.read_u8().unwrap() as f32 * scale[1] + origin[1],
                        reader.read_u8().unwrap() as f32 * scale[2] + origin[2],
                    ));
                    reader.read_u8().unwrap(); // discard vertex normal
                }

                Frame::Single(FrameSingle {
                    min,
                    max,
                    name,
                    vertices: vertices.into_boxed_slice(),
                })
            }

            1 => {
                let subframe_count = match reader.read_i32::<LittleEndian>().unwrap() {
                    s if s <= 0 => panic!("Invalid subframe count: {}", s),
                    s => s,
                };

                let abs_min = Vector3::new(
                    reader.read_u8().unwrap() as f32 * scale[0] + origin[0],
                    reader.read_u8().unwrap() as f32 * scale[1] + origin[1],
                    reader.read_u8().unwrap() as f32 * scale[2] + origin[2],
                );

                reader.read_u8().unwrap(); // discard vertex normal

                let abs_max = Vector3::new(
                    reader.read_u8().unwrap() as f32 * scale[0] + origin[0],
                    reader.read_u8().unwrap() as f32 * scale[1] + origin[1],
                    reader.read_u8().unwrap() as f32 * scale[2] + origin[2],
                );

                reader.read_u8().unwrap(); // discard vertex normal

                let mut intervals = Vec::new();
                for _ in 0..subframe_count {
                    intervals.push(engine::duration_from_f32(
                        reader.read_f32::<LittleEndian>().unwrap(),
                    ));
                }

                let mut subframes = Vec::new();
                for _ in 0..subframe_count {
                    let min = Vector3::new(
                        reader.read_u8().unwrap() as f32 * scale[0] + origin[0],
                        reader.read_u8().unwrap() as f32 * scale[1] + origin[1],
                        reader.read_u8().unwrap() as f32 * scale[2] + origin[2],
                    );

                    reader.read_u8().unwrap(); // discard vertex normal

                    let max = Vector3::new(
                        reader.read_u8().unwrap() as f32 * scale[0] + origin[0],
                        reader.read_u8().unwrap() as f32 * scale[1] + origin[1],
                        reader.read_u8().unwrap() as f32 * scale[2] + origin[2],
                    );

                    reader.read_u8().unwrap(); // discard vertex normal

                    let name = {
                        let mut bytes: [u8; 16] = [0; 16];
                        reader.read(&mut bytes).unwrap();
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

                    let mut vertices: Vec<Vector3<f32>> = Vec::with_capacity(vertex_count as usize);
                    for _ in 0..vertex_count {
                        vertices.push(Vector3::new(
                            reader.read_u8().unwrap() as f32 * scale[0] + origin[0],
                            reader.read_u8().unwrap() as f32 * scale[1] + origin[1],
                            reader.read_u8().unwrap() as f32 * scale[2] + origin[2],
                        ));
                        reader.read_u8().unwrap(); // discard vertex normal
                    }

                    subframes.push(FrameSingle {
                        min,
                        max,
                        name,
                        vertices: vertices.into_boxed_slice(),
                    })
                }

                Frame::Group(FrameGroup {
                    min: abs_min,
                    max: abs_max,
                    times: intervals,
                    frames: subframes,
                })
            }

            x => panic!("Bad frame kind value: {}", x),
        });
    }

    if reader.seek(SeekFrom::Current(0)).unwrap() != reader.seek(SeekFrom::End(0)).unwrap() {
        panic!("Misaligned read on MDL file");
    }

    Ok(AliasModel {
        origin: origin,
        radius: radius,
        skins: skins.into_boxed_slice(),
        texcoords: texcoords.into_boxed_slice(),
        indices: indices.into_boxed_slice(),
        frames: frames.into_boxed_slice(),
    })
}
