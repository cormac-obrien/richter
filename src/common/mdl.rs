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

use std::io::{self, BufReader, Read, Seek, SeekFrom};

use crate::common::{
    engine,
    model::{ModelFlags, SyncType},
    util::read_f32_3,
};

use byteorder::{LittleEndian, ReadBytesExt};
use cgmath::{ElementWise as _, Vector3};
use chrono::Duration;
use num::FromPrimitive;
use thiserror::Error;

#[allow(clippy::identity_op)]
pub const MAGIC: i32 =
    ('I' as i32) << 0 | ('D' as i32) << 8 | ('P' as i32) << 16 | ('O' as i32) << 24;
pub const VERSION: i32 = 6;

const HEADER_SIZE: u64 = 84;

#[derive(Error, Debug)]
pub enum MdlFileError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("Invalid magic number: found {0}, expected {}", MAGIC)]
    InvalidMagicNumber(i32),
    #[error("Unrecognized version: {0}")]
    UnrecognizedVersion(i32),
    #[error("Invalid texture width: {0}")]
    InvalidTextureWidth(i32),
    #[error("Invalid texture height: {0}")]
    InvalidTextureHeight(i32),
    #[error("Invalid vertex count: {0}")]
    InvalidVertexCount(i32),
    #[error("Invalid polygon count: {0}")]
    InvalidPolygonCount(i32),
    #[error("Invalid keyframe count: {0}")]
    InvalidKeyframeCount(i32),
    #[error("Invalid model flags: {0:X?}")]
    InvalidFlags(i32),
    #[error("Invalid texture kind: {0}")]
    InvalidTextureKind(i32),
    #[error("Invalid seam flag: {0}")]
    InvalidSeamFlag(i32),
    #[error("Invalid texture coordinates: {0:?}")]
    InvalidTexcoord([i32; 2]),
    #[error("Invalid front-facing flag: {0}")]
    InvalidFrontFacing(i32),
    #[error("Keyframe name too long: {0:?}")]
    KeyframeNameTooLong([u8; 16]),
    #[error("Non-UTF-8 keyframe name: {0}")]
    NonUtf8KeyframeName(#[from] std::string::FromUtf8Error),
}

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

pub fn load<R>(data: R) -> Result<AliasModel, MdlFileError>
where
    R: Read + Seek,
{
    let mut reader = BufReader::new(data);

    // struct MdlHeader {
    //     magic: i32
    //     version: i32
    //     scale: [f32; 3]
    //     origin: [f32; 3]
    //     radius: f32
    //     eye_position: [f32; 3]
    //     texture_count: i32,
    //     texture_width: i32,
    //     texture_height: i32,
    //     vertex_count: i32,
    //     poly_count: i32,
    //     keyframe_count: i32,
    //     sync_type: i32,
    //     flags_bits: i32,
    // }

    let magic = reader.read_i32::<LittleEndian>()?;
    if magic != MAGIC {
        return Err(MdlFileError::InvalidMagicNumber(magic));
    }

    let version = reader.read_i32::<LittleEndian>()?;
    if version != VERSION {
        return Err(MdlFileError::UnrecognizedVersion(version));
    }

    let scale: Vector3<f32> = read_f32_3(&mut reader)?.into();
    let origin: Vector3<f32> = read_f32_3(&mut reader)?.into();
    let radius = reader.read_f32::<LittleEndian>()?;
    let _eye_position: Vector3<f32> = read_f32_3(&mut reader)?.into();
    let texture_count = reader.read_i32::<LittleEndian>()?;
    let texture_width = reader.read_i32::<LittleEndian>()?;
    if texture_width <= 0 {
        return Err(MdlFileError::InvalidTextureWidth(texture_width));
    }
    let texture_height = reader.read_i32::<LittleEndian>()?;
    if texture_height <= 0 {
        return Err(MdlFileError::InvalidTextureHeight(texture_height));
    }
    let vertex_count = reader.read_i32::<LittleEndian>()?;
    if vertex_count <= 0 {
        return Err(MdlFileError::InvalidVertexCount(vertex_count));
    }
    let poly_count = reader.read_i32::<LittleEndian>()?;
    if poly_count <= 0 {
        return Err(MdlFileError::InvalidPolygonCount(poly_count));
    }
    let keyframe_count = reader.read_i32::<LittleEndian>()?;
    if keyframe_count <= 0 {
        return Err(MdlFileError::InvalidKeyframeCount(keyframe_count));
    }

    let _sync_type = SyncType::from_i32(reader.read_i32::<LittleEndian>()?);

    let flags_bits = reader.read_i32::<LittleEndian>()?;
    if flags_bits < 0 || flags_bits > u8::MAX as i32 {
        return Err(MdlFileError::InvalidFlags(flags_bits));
    }
    let flags =
        ModelFlags::from_bits(flags_bits as u8).ok_or(MdlFileError::InvalidFlags(flags_bits))?;

    // unused
    let _size = reader.read_i32::<LittleEndian>()?;

    assert_eq!(
        reader.seek(SeekFrom::Current(0))?,
        reader.seek(SeekFrom::Start(HEADER_SIZE))?,
        "Misaligned read on MDL header"
    );

    let mut textures: Vec<Texture> = Vec::with_capacity(texture_count as usize);

    for _ in 0..texture_count {
        // TODO: add a TextureKind type
        let texture = match reader.read_i32::<LittleEndian>()? {
            // Static
            0 => {
                let mut indices: Vec<u8> =
                    Vec::with_capacity((texture_width * texture_height) as usize);
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
                    let mut indices: Vec<u8> =
                        Vec::with_capacity((texture_width * texture_height) as usize);
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

            k => return Err(MdlFileError::InvalidTextureKind(k)),
        };

        textures.push(texture);
    }

    let mut texcoords = Vec::with_capacity(vertex_count as usize);
    for _ in 0..vertex_count {
        let is_on_seam = match reader.read_i32::<LittleEndian>()? {
            0 => false,
            0x20 => true,
            x => return Err(MdlFileError::InvalidSeamFlag(x)),
        };

        let s = reader.read_i32::<LittleEndian>()?;
        let t = reader.read_i32::<LittleEndian>()?;
        if s < 0 || t < 0 {
            return Err(MdlFileError::InvalidTexcoord([s, t]));
        }

        texcoords.push(Texcoord {
            is_on_seam,
            s: s as u32,
            t: t as u32,
        });
    }

    let mut polygons = Vec::with_capacity(poly_count as usize);
    for _ in 0..poly_count {
        let faces_front = match reader.read_i32::<LittleEndian>()? {
            0 => false,
            1 => true,
            x => return Err(MdlFileError::InvalidFrontFacing(x)),
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
                let min = read_vertex(&mut reader, scale, origin)?;
                reader.read_u8()?; // discard vertex normal
                let max = read_vertex(&mut reader, scale, origin)?;
                reader.read_u8()?; // discard vertex normal

                let name = {
                    let mut bytes: [u8; 16] = [0; 16];
                    reader.read_exact(&mut bytes)?;
                    let len = bytes
                        .iter()
                        .position(|b| *b == 0)
                        .ok_or(MdlFileError::KeyframeNameTooLong(bytes))?;
                    String::from_utf8(bytes[0..(len + 1)].to_vec())?
                };

                debug!("Keyframe name: {}", name);

                let mut vertices: Vec<Vector3<f32>> = Vec::with_capacity(vertex_count as usize);
                for _ in 0..vertex_count {
                    vertices.push(read_vertex(&mut reader, scale, origin)?);
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

                let abs_min = read_vertex(&mut reader, scale, origin)?;
                reader.read_u8()?; // discard vertex normal
                let abs_max = read_vertex(&mut reader, scale, origin)?;
                reader.read_u8()?; // discard vertex normal

                let mut durations = Vec::new();
                for _ in 0..subframe_count {
                    durations.push(engine::duration_from_f32(
                        reader.read_f32::<LittleEndian>()?,
                    ));
                }

                let mut subframes = Vec::new();
                for subframe_id in 0..subframe_count {
                    let min = read_vertex(&mut reader, scale, origin)?;
                    reader.read_u8()?; // discard vertex normal
                    let max = read_vertex(&mut reader, scale, origin)?;
                    reader.read_u8()?; // discard vertex normal

                    let name = {
                        let mut bytes: [u8; 16] = [0; 16];
                        reader.read_exact(&mut bytes)?;
                        let len = bytes
                            .iter()
                            .position(|b| *b == 0)
                            .ok_or(MdlFileError::KeyframeNameTooLong(bytes))?;
                        String::from_utf8(bytes[0..(len + 1)].to_vec())?
                    };

                    debug!("Frame name: {}", name);

                    let mut vertices: Vec<Vector3<f32>> = Vec::with_capacity(vertex_count as usize);
                    for _ in 0..vertex_count {
                        vertices.push(read_vertex(&mut reader, scale, origin)?);
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

fn read_vertex<R>(
    reader: &mut R,
    scale: Vector3<f32>,
    translate: Vector3<f32>,
) -> Result<Vector3<f32>, io::Error>
where
    R: ReadBytesExt,
{
    Ok(Vector3::new(
        reader.read_u8()? as f32,
        reader.read_u8()? as f32,
        reader.read_u8()? as f32,
    )
    .mul_element_wise(scale)
        + translate)
}
