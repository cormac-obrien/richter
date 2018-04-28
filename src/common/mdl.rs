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

use std::io::BufReader;
use std::io::Cursor;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;

use common::engine;
use common::model::{ModelFlags, SyncType};

use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use cgmath::Vector3;
use chrono::Duration;
use failure::Error;
use num::FromPrimitive;

pub const MAGIC: i32 =
    ('I' as i32) << 0 | ('D' as i32) << 8 | ('P' as i32) << 16 | ('O' as i32) << 24;
pub const VERSION: i32 = 6;

const HEADER_SIZE: u64 = 84;

#[derive(Clone, Debug)]
pub struct StaticTexture {
    indices: Box<[u8]>,
}

impl StaticTexture {
    /// Returns the indexed colors of this texture.
    pub fn indices(&self) -> &[u8] {
        &self.indices
    }
}

#[derive(Clone, Debug)]
pub struct AnimatedTextureFrame {
    duration: Duration,
    indices: Box<[u8]>,
}

impl AnimatedTextureFrame {
    /// Returns the duration of this frame.
    pub fn duration(&self) -> Duration {
        self.duration
    }

    /// Returns the indexed colors of this texture.
    pub fn indices(&self) -> &[u8] {
        &self.indices
    }
}

#[derive(Clone, Debug)]
pub struct AnimatedTexture {
    frames: Box<[AnimatedTextureFrame]>,
}

impl AnimatedTexture {
    pub fn frames(&self) -> &[AnimatedTextureFrame] {
        &self.frames
    }
}

#[derive(Clone, Debug)]
pub enum Texture {
    Static(StaticTexture),
    Animated(AnimatedTexture),
}

#[derive(Clone, Debug)]
pub struct Texcoord {
    is_on_seam: bool,
    s: u32,
    t: u32,
}

impl Texcoord {
    pub fn is_on_seam(&self) -> bool {
        self.is_on_seam
    }

    pub fn s(&self) -> u32 {
        self.s
    }

    pub fn t(&self) -> u32 {
        self.t
    }
}

#[derive(Clone, Debug)]
pub struct IndexedPolygon {
    faces_front: bool,
    indices: [u32; 3],
}

impl IndexedPolygon {
    pub fn faces_front(&self) -> bool {
        self.faces_front
    }

    pub fn indices(&self) -> &[u32; 3] {
        &self.indices
    }
}

#[derive(Clone, Debug)]
pub struct StaticKeyframe {
    name: String,
    min: Vector3<f32>,
    max: Vector3<f32>,
    vertices: Box<[Vector3<f32>]>,
}

impl StaticKeyframe {
    /// Returns the name of this keyframe.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the minimum extent of this keyframe relative to the model origin.
    pub fn min(&self) -> Vector3<f32> {
        self.min
    }

    /// Returns the minimum extent of this keyframe relative to the model origin.
    pub fn max(&self) -> Vector3<f32> {
        self.max
    }

    /// Returns the vertices defining this keyframe.
    pub fn vertices(&self) -> &[Vector3<f32>] {
        &self.vertices
    }
}

#[derive(Clone, Debug)]
pub struct AnimatedKeyframeFrame {
    name: String,
    min: Vector3<f32>,
    max: Vector3<f32>,
    duration: Duration,
    vertices: Box<[Vector3<f32>]>,
}

impl AnimatedKeyframeFrame {
    /// Returns the name of this subframe.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the minimum extent of this keyframe relative to the model origin.
    pub fn min(&self) -> Vector3<f32> {
        self.min
    }

    /// Returns the minimum extent of this keyframe relative to the model origin.
    pub fn max(&self) -> Vector3<f32> {
        self.max
    }

    /// Returns the duration of this subframe.
    pub fn duration(&self) -> Duration {
        self.duration
    }

    /// Returns the vertices defining this subframe.
    pub fn vertices(&self) -> &[Vector3<f32>] {
        &self.vertices
    }
}

#[derive(Clone, Debug)]
pub struct AnimatedKeyframe {
    min: Vector3<f32>,
    max: Vector3<f32>,
    frames: Box<[AnimatedKeyframeFrame]>,
}

impl AnimatedKeyframe {
    /// Returns the minimum extent of all subframes in this keyframe relative to the model origin.
    pub fn min(&self) -> Vector3<f32> {
        self.min
    }

    /// Returns the maximum extent of all subframes in this keyframe relative to the model origin.
    pub fn max(&self) -> Vector3<f32> {
        self.max
    }

    /// Returns the subframes of this keyframe.
    pub fn frames(&self) -> &[AnimatedKeyframeFrame] {
        &self.frames
    }
}

#[derive(Clone, Debug)]
pub enum Keyframe {
    Static(StaticKeyframe),
    Animated(AnimatedKeyframe),
}

#[derive(Debug)]
pub struct AliasModel {
    origin: Vector3<f32>,
    radius: f32,
    texture_width: u32,
    texture_height: u32,
    textures: Box<[Texture]>,
    texcoords: Box<[Texcoord]>,
    polygons: Box<[IndexedPolygon]>,
    keyframes: Box<[Keyframe]>,
    flags: ModelFlags,
}

impl AliasModel {
    pub fn origin(&self) -> Vector3<f32> {
        self.origin
    }

    pub fn radius(&self) -> f32 {
        self.radius
    }

    pub fn texture_width(&self) -> u32 {
        self.texture_width
    }

    pub fn texture_height(&self) -> u32 {
        self.texture_height
    }

    pub fn textures(&self) -> &[Texture] {
        &self.textures
    }

    pub fn texcoords(&self) -> &[Texcoord] {
        &self.texcoords
    }

    pub fn polygons(&self) -> &[IndexedPolygon] {
        &self.polygons
    }

    pub fn keyframes(&self) -> &[Keyframe] {
        &self.keyframes
    }

    pub fn flags(&self) -> ModelFlags {
        self.flags
    }
}

pub fn load(data: &[u8]) -> Result<AliasModel, Error> {
    let mut reader = BufReader::new(Cursor::new(data));

    let magic = reader.read_i32::<LittleEndian>()?;
    ensure!(magic == MAGIC, "Bad MDL magic number (got {}, should be {})", magic, MAGIC);

    let version = reader.read_i32::<LittleEndian>()?;
    ensure!(version == VERSION, "Bad MDL version (got {}, should be {})", version, VERSION);

    let scale = Vector3::new(
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?,
    );

    let origin = Vector3::new(
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?,
    );

    let radius = reader.read_f32::<LittleEndian>()?;

    let eye_position = Vector3::new(
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?,
        reader.read_f32::<LittleEndian>()?,
    );

    let texture_count = reader.read_i32::<LittleEndian>()?;

    let texture_width = reader.read_i32::<LittleEndian>()?;
    ensure!(texture_width > 0, "Texture width must be positive (got {})", texture_width);

    let texture_height = reader.read_i32::<LittleEndian>()?;
    ensure!(texture_height > 0, "Texture height must be positive (got {})", texture_height);

    let vertex_count = reader.read_i32::<LittleEndian>()?;
    ensure!(vertex_count > 0, "Vertex count must be positive (got {})", vertex_count);

    let poly_count = reader.read_i32::<LittleEndian>()?;
    ensure!(poly_count > 0, "Poly count must be positive (got {})", poly_count);

    let keyframe_count = reader.read_i32::<LittleEndian>()?;
    ensure!(keyframe_count > 0, "Keyframe count must be positive (got {})", keyframe_count);

    let sync_type = SyncType::from_i32(reader.read_i32::<LittleEndian>()?);

    let flags_bits = reader.read_i32::<LittleEndian>()?;
    ensure!(flags_bits >= 0, "Invalid flag bits for alias model");
    ensure!(flags_bits < ::std::u8::MAX as i32, "Invalid flag bits for alias model");
    let flags = ModelFlags::from_bits(flags_bits as u8).unwrap();

    let size = match reader.read_i32::<LittleEndian>()? {
        s if s < 0 => panic!("Negative size ({})", s),
        s => s,
    };

    ensure!(
        reader.seek(SeekFrom::Current(0))? == reader.seek(SeekFrom::Start(HEADER_SIZE))?,
        "Misaligned read on MDL header"
    );

    let mut textures: Vec<Texture> = Vec::with_capacity(texture_count as usize);

    for _ in 0..texture_count {
        // TODO: add a TextureKind type
        let texture = match reader.read_i32::<LittleEndian>()? {
            // Static
            0 => {
                let mut indices: Vec<u8> = Vec::with_capacity((texture_width * texture_height) as usize);
                (&mut reader)
                    .take((texture_width * texture_height) as u64)
                    .read_to_end(&mut indices)?;
                Texture::Static(StaticTexture {
                    indices: indices.into_boxed_slice(),
                })
            }

            // Animated
            1 => {
                // TODO: sanity check this value
                let texture_frame_count = reader.read_i32::<LittleEndian>()? as usize;

                let mut durations = Vec::with_capacity(texture_frame_count);
                for _ in 0..texture_frame_count {
                    durations.push(engine::duration_from_f32(
                        reader.read_f32::<LittleEndian>()?,
                    ));
                }

                let mut frames = Vec::with_capacity(texture_frame_count);
                for frame_id in 0..texture_frame_count {
                    let mut indices: Vec<u8> = Vec::with_capacity((texture_width * texture_height) as usize);
                    (&mut reader)
                        .take((texture_width * texture_height) as u64)
                        .read_to_end(&mut indices)?;
                    frames.push(AnimatedTextureFrame {
                        duration: durations[frame_id as usize],
                        indices: indices.into_boxed_slice(),
                    });
                }

                Texture::Animated(AnimatedTexture {
                    frames: frames.into_boxed_slice(),
                })
            }

            _ => panic!("Bad texture type"),
        };

        textures.push(texture);
    }

    // NOTE:
    // For the time being, texture coordinate adjustment for vertices which are
    //   1) on the seam, and
    //   2) part of a rear-facing poly
    // is being ignored. This process is optimized in the MDL format for OpenGL immediate mode
    // and I haven't found an elegant way to implement it yet.

    let mut texcoords = Vec::with_capacity(vertex_count as usize);
    for _ in 0..vertex_count {
        let is_on_seam = match reader.read_i32::<LittleEndian>()? {
            0 => false,
            0x20 => true,
            x => bail!("bad seam value: {}", x),
        };

        let s = reader.read_i32::<LittleEndian>()?;
        let t = reader.read_i32::<LittleEndian>()?;
        ensure!(s >= 0, "Negative s value: {}", s);
        ensure!(t >= 0, "Negative t value: {}", t);

        texcoords.push(Texcoord {
            is_on_seam,
            s: s as u32,
            t: t as u32,
        });
    }

    // let mut poly_facings: Vec<bool> = Vec::with_capacity(poly_count as usize);
    let mut polygons = Vec::with_capacity(poly_count as usize);
    for _ in 0..poly_count {
        let faces_front = match reader.read_i32::<LittleEndian>()? {
            0 => false,
            1 => true,
            x => bail!("bad faces_front value: {}", x),
        };

        let mut indices = [0; 3];
        for i in 0..3 {
            indices[i] = reader.read_i32::<LittleEndian>()? as u32;
        }

        polygons.push(IndexedPolygon {
            faces_front,
            indices,
        });
    }

    let mut keyframes: Vec<Keyframe> = Vec::with_capacity(keyframe_count as usize);
    for _ in 0..keyframe_count {
        keyframes.push(match reader.read_i32::<LittleEndian>()? {
            0 => {
                let min = Vector3::new(
                    reader.read_u8()? as f32 * scale[0] + origin[0],
                    reader.read_u8()? as f32 * scale[1] + origin[1],
                    reader.read_u8()? as f32 * scale[2] + origin[2],
                );

                reader.read_u8()?; // discard vertex normal

                let max = Vector3::new(
                    reader.read_u8()? as f32 * scale[0] + origin[0],
                    reader.read_u8()? as f32 * scale[1] + origin[1],
                    reader.read_u8()? as f32 * scale[2] + origin[2],
                );

                reader.read_u8()?; // discard vertex normal

                let name = {
                    let mut bytes: [u8; 16] = [0; 16];
                    reader.read(&mut bytes)?;
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
                    String::from_utf8(bytes[0..(len + 1)].to_vec())?
                };

                debug!("Keyframe name: {}", name);

                let mut vertices: Vec<Vector3<f32>> = Vec::with_capacity(vertex_count as usize);
                for _ in 0..vertex_count {
                    vertices.push(Vector3::new(
                        reader.read_u8()? as f32 * scale[0] + origin[0],
                        reader.read_u8()? as f32 * scale[1] + origin[1],
                        reader.read_u8()? as f32 * scale[2] + origin[2],
                    ));
                    reader.read_u8()?; // discard vertex normal
                }

                Keyframe::Static(StaticKeyframe {
                    name,
                    min,
                    max,
                    vertices: vertices.into_boxed_slice(),
                })
            }

            1 => {
                let subframe_count = match reader.read_i32::<LittleEndian>()? {
                    s if s <= 0 => panic!("Invalid subframe count: {}", s),
                    s => s,
                };

                let abs_min = Vector3::new(
                    reader.read_u8()? as f32 * scale[0] + origin[0],
                    reader.read_u8()? as f32 * scale[1] + origin[1],
                    reader.read_u8()? as f32 * scale[2] + origin[2],
                );

                reader.read_u8()?; // discard vertex normal

                let abs_max = Vector3::new(
                    reader.read_u8()? as f32 * scale[0] + origin[0],
                    reader.read_u8()? as f32 * scale[1] + origin[1],
                    reader.read_u8()? as f32 * scale[2] + origin[2],
                );

                reader.read_u8()?; // discard vertex normal

                let mut durations = Vec::new();
                for _ in 0..subframe_count {
                    durations.push(engine::duration_from_f32(
                        reader.read_f32::<LittleEndian>()?,
                    ));
                }

                let mut subframes = Vec::new();
                for subframe_id in 0..subframe_count {
                    let min = Vector3::new(
                        reader.read_u8()? as f32 * scale[0] + origin[0],
                        reader.read_u8()? as f32 * scale[1] + origin[1],
                        reader.read_u8()? as f32 * scale[2] + origin[2],
                    );

                    reader.read_u8()?; // discard vertex normal

                    let max = Vector3::new(
                        reader.read_u8()? as f32 * scale[0] + origin[0],
                        reader.read_u8()? as f32 * scale[1] + origin[1],
                        reader.read_u8()? as f32 * scale[2] + origin[2],
                    );

                    reader.read_u8()?; // discard vertex normal

                    let mut name_bytes: [u8; 16] = [0; 16];
                    reader.read(&mut name_bytes)?;
                    let mut name_len = None;
                    for byte_id in 0..name_bytes.len() {
                        if name_bytes[byte_id] == 0 {
                            name_len = Some(byte_id);
                            break;
                        }
                    }

                    let name = match name_len {
                        Some(n) => String::from_utf8(name_bytes[..n].to_vec())?,
                        None => bail!("Invalid subframe name"),
                    };

                    debug!("Frame name: {}", name);

                    let mut vertices: Vec<Vector3<f32>> = Vec::with_capacity(vertex_count as usize);
                    for _ in 0..vertex_count {
                        vertices.push(Vector3::new(
                            reader.read_u8()? as f32 * scale[0] + origin[0],
                            reader.read_u8()? as f32 * scale[1] + origin[1],
                            reader.read_u8()? as f32 * scale[2] + origin[2],
                        ));
                        reader.read_u8()?; // discard vertex normal
                    }

                    subframes.push(AnimatedKeyframeFrame {
                        min,
                        max,
                        name,
                        duration: durations[subframe_id as usize],
                        vertices: vertices.into_boxed_slice(),
                    })
                }

                Keyframe::Animated(AnimatedKeyframe {
                    min: abs_min,
                    max: abs_max,
                    frames: subframes.into_boxed_slice(),
                })
            }

            x => panic!("Bad frame kind value: {}", x),
        });
    }

    if reader.seek(SeekFrom::Current(0))? != reader.seek(SeekFrom::End(0))? {
        panic!("Misaligned read on MDL file");
    }

    Ok(AliasModel {
        origin,
        radius,
        texture_width: texture_width as u32,
        texture_height: texture_height as u32,
        textures: textures.into_boxed_slice(),
        texcoords: texcoords.into_boxed_slice(),
        polygons: polygons.into_boxed_slice(),
        keyframes: keyframes.into_boxed_slice(),
        flags,
    })
}
