use std::convert::From;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io;
use std::io::Read;
use std::path::Path;
use std::string::FromUtf8Error;

use byteorder::{LittleEndian, ReadBytesExt};

const MAGIC: i32 = 0x4F504449;
const VERSION: i32 = 6;

// TODO: create more informative errors
#[derive(Debug)]
pub enum MdlError {
    Io(io::Error),
    Utf8(FromUtf8Error),
    Value,
    Other,
}

impl fmt::Display for MdlError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

impl Error for MdlError {
    fn description(&self) -> &str {
        match *self {
            MdlError::Io(_) => "I/O error",
            MdlError::Utf8(_) => "Utf-8 decoding error",
            MdlError::Value => "Value error",
            MdlError::Other => "Unknown error",
        }
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            MdlError::Io(ref i) => Some(i),
            MdlError::Utf8(ref i) => Some(i),
            MdlError::Value => None,
            MdlError::Other => None,
        }
    }
}

impl From<io::Error> for MdlError {
    fn from(err: io::Error) -> MdlError {
        MdlError::Io(err)
    }
}

impl From<FromUtf8Error> for MdlError {
    fn from(err: FromUtf8Error) -> MdlError {
        MdlError::Utf8(err)
    }
}

// TODO: implement serialization

//
pub struct Header {
    // MDL file magic number. Must be equal to mdl::MAGIC.
    pub magic: i32,

    // MDL file version number. Must be equal to mdl::VERSION.
    pub version: i32,

    // The scaling factor of this model. This is used to calculate the actual vertex positions
    // from their packed representations.
    pub scale: [f32; 3],

    // The origin of this model, used to translate vertices to their correct positions after
    // unpacking.
    pub origin: [f32; 3],

    // The bounding radius of this model.
    pub radius: f32,

    // The location of the eyes on this model. This is not used in the Quake engine.
    pub eyes: [f32; 3],

    // The total number of skins (textures) associated with this model.
    pub skin_count: i32,

    // The width in pixels of each skin.
    pub skin_w: i32,

    // The height in pixels of each skin.
    pub skin_h: i32,
    pub vertex_count: i32,
    pub triangle_count: i32,
    pub frame_count: i32,
    pub sync_type: i32,
    pub flags: i32,
    pub size: f32,
}

// A single skin for the model.
pub struct SkinSingle {
    // An array of color palette indices.
    pub indices: Vec<u8>,
}

// One frame of an animated skin.
pub struct SkinTimed {
    pub time: f32,
    pub indices: Vec<u8>,
}

// A group of skins for the model forming an animation.
pub struct SkinGroup {
    pub skins: Vec<SkinTimed>,
}

impl SkinGroup {
    pub fn len(&self) -> usize {
        self.skins.len()
    }
}

// Sum type for skins.
pub enum Skin {
    Single(SkinSingle),
    Group(SkinGroup),
}

// A texture coordinate.
pub struct Texcoord {
    // Whether or not this coordinate falls on the 'seam' between the front and back of the
    // model.
    pub seam: bool,

    // The horizontal component.
    pub s: u32,

    // The vertical component.
    pub t: u32,
}

pub struct Texcoords {
    pub seams: Vec<bool>,
    pub texcoords: Vec<u32>,
}

pub struct Triangles {
    pub fronts: Vec<bool>,
    pub indices: Vec<u32>,
}

// A packed vertex.
pub struct Vert {
    // The packed position of this vertex. This will be multiplied component-wise with the
    // model's scaling factor to produce the final position.
    pub pos: [u8; 3],

    // The index of this vertex's precalculated normal vector. This normal vector is provided
    // by the Quake engine for Gouraud shading.
    pub normal: u8,
}

// A single model animation frame.
pub struct FrameSingle {
    // The minimum extent in space of this frame.
    pub min: Vert,

    // The maximum extent in space of this frame.
    pub max: Vert,

    // The name of this frame.
    pub name: String,

    // The vertices that comprise this frame.
    pub data: Vec<Vert>,
}

pub struct FrameTimed {
    // The length in seconds of this frame.
    pub time: f32,

    // The frame.
    pub frame: FrameSingle,
}

// A group of model animation frames comprising a sub-animation.
pub struct FrameGroup {
    // The minimum extent in space of this frame.
    pub min: Vert,

    // The maximum extent in space of this frame.
    pub max: Vert,

    // The timed frames comprising this group.
    pub frames: Vec<FrameTimed>,
}

impl FrameGroup {
    pub fn len(&self) -> usize {
        self.frames.len()
    }
}

// Sum type for frames.
pub enum Frame {
    Single(FrameSingle),
    Group(FrameGroup),
}

pub struct Mdl {
    pub header: Header,
    pub skins: Vec<Skin>,
    pub texcoords: Vec<Texcoord>,
    pub triangles: Triangles,
    pub frames: Vec<Frame>,
}

impl Mdl {
    fn check_header(header: &Header) -> Result<(), MdlError> {
        if header.magic != MAGIC {
            println!("Bad magic number");
            return Err(MdlError::Value);
        }

        if header.version != VERSION {
            println!("Bad version number");
            return Err(MdlError::Value);
        }

        if header.skin_count <= 0 {
            println!("Bad skin count");
            return Err(MdlError::Value);
        }

        // The Quake engine performs this check to ensure skin data is aligned properly
        if header.skin_w <= 0 || header.skin_w % 4 != 0 {
            println!("Bad skin width");
            return Err(MdlError::Value);
        }

        if header.vertex_count <= 0 {
            println!("Bad vertex count");
            return Err(MdlError::Value);
        }

        if header.triangle_count <= 0 {
            println!("Bad triangle count");
            return Err(MdlError::Value);
        }

        if header.frame_count <= 0 {
            println!("Bad frame count");
            return Err(MdlError::Value);
        }

        Ok(())
    }

    fn load_skins(header: &Header,
                  file: &mut File)
                  -> Result<Vec<Skin>, MdlError> {
        let pixel_count = (header.skin_h * header.skin_w) as usize;
        let mut skins: Vec<Skin> = Vec::with_capacity(header.skin_count as usize);
        for _ in 0..header.skin_count {
            let is_group = try!(file.read_i32::<LittleEndian>());
            let skin: Skin = match is_group {
                // single skin
                0 => {
                    let mut indices: Vec<u8> = Vec::with_capacity(pixel_count);
                    try!(file.take(pixel_count as u64).read_to_end(&mut indices));
                    Skin::Single(SkinSingle { indices: indices})
                }

                // group skin
                1 => {
                    let count = {
                        let _count = try!(file.read_i32::<LittleEndian>());
                        if _count <= 0 { // TODO: specify maximum count
                            return Err(MdlError::Value);
                        }
                        _count as usize
                    };

                    let mut skins: Vec<SkinTimed> = Vec::with_capacity(header.frame_count as usize);
                    for _ in 0..(header.frame_count as usize) {
                        let time = {
                            let _time = try!(file.read_f32::<LittleEndian>());
                            if _time <= 0.0 {
                                return Err(MdlError::Value);
                            }
                            _time
                        };

                        let mut indices = Vec::with_capacity((pixel_count * count) as usize);
                        try!(file.take((pixel_count * count) as u64).read_to_end(&mut indices));

                        skins.push(SkinTimed {
                            time: time,
                            indices: indices,
                        });
                    }

                    Skin::Group(SkinGroup { skins: skins })
                }

                _ => return Err(MdlError::Value),
            };
            skins.push(skin);
        }
        Ok(skins)
    }

    fn load_texcoords(header: &Header, file: &mut File) -> Result<Vec<Texcoord>, MdlError> {
        let mut texcoords: Vec<Texcoord> = Vec::with_capacity(header.vertex_count as usize);
        for _ in 0..header.vertex_count {
            texcoords.push(Texcoord {
                seam: match try!(file.read_i32::<LittleEndian>()) {
                    0 => false,
                    0x20 => true,
                    _ => return Err(MdlError::Value),
                },
                s: {
                    let _s = try!(file.read_i32::<LittleEndian>());
                    if _s < 0 {
                        return Err(MdlError::Value);
                    }
                    _s as u32
                },
                t: {
                    let _t = try!(file.read_i32::<LittleEndian>());
                    if _t < 0 {
                        return Err(MdlError::Value);
                    }
                    _t as u32
                },
            });
        }
        Ok(texcoords)
    }

    fn load_triangles(header: &Header, file: &mut File) -> Result<Triangles, MdlError> {
        let mut triangles = Triangles {
            fronts: Vec::with_capacity(header.triangle_count as usize),
            indices: Vec::with_capacity((header.triangle_count * 3) as usize),
        };

        for _ in 0..header.triangle_count {
            triangles.fronts.push(match try!(file.read_i32::<LittleEndian>()) {
                0 => false,
                1 => true,
                _ => return Err(MdlError::Value),
            });

            for i in 0..3 {
                let c = try!(file.read_i32::<LittleEndian>());
                if c < 0 {
                    return Err(MdlError::Value);
                }
                triangles.indices.push(c as u32);
            }
        }

        Ok(triangles)
    }

    fn load_vertex(header: &Header, file: &mut File) -> Result<Vert, MdlError> {
        let vert = Vert {
            pos: {
                let mut _pos: [u8; 3] = [0; 3];
                for i in 0..3 {
                    _pos[i] = try!(file.read_u8()) as u8;
                }
                _pos
            },
            normal: try!(file.read_u8()),
        };
        Ok(vert)
    }

    fn load_framesingle(header: &Header, file: &mut File) -> Result<FrameSingle, MdlError> {
        let min = try!(Mdl::load_vertex(header, file));
        let max = try!(Mdl::load_vertex(header, file));
        let name = {
            let mut bytes: [u8; 16] = [0; 16];
            try!(file.read(&mut bytes));
            let len = {
                let mut _len: usize = 0;
                for i in 0..16 {
                    if bytes[i] == 0 {
                        break;
                    }
                    _len += 1;
                }
                _len
            };
            try!(String::from_utf8(bytes[0..len].to_vec()))
        };
        let data = {
            let mut _data: Vec<Vert> = Vec::with_capacity(header.vertex_count as usize);
            for _ in 0..header.vertex_count {
                _data.push(try!(Mdl::load_vertex(header, file)));
            }
            _data
        };

        Ok(FrameSingle {
            min: min,
            max: max,
            name: name,
            data: data,
        })
    }

    fn load_frames(header: &Header, file: &mut File) -> Result<Vec<Frame>, MdlError> {
        let mut frames: Vec<Frame> = Vec::with_capacity(header.frame_count as usize);
        for _ in 0..header.frame_count {
            let is_group = try!(file.read_i32::<LittleEndian>());
            let frame: Frame = match is_group {
                0 => Frame::Single(try!(Mdl::load_framesingle(header, file))),
                _ => {
                    let count = {
                        let _count = try!(file.read_i32::<LittleEndian>());
                        if _count <= 0 {
                            return Err(MdlError::Value);
                        }
                        _count as u32
                    };

                    let min = try!(Mdl::load_vertex(header, file));
                    let max = try!(Mdl::load_vertex(header, file));

                    let frames = {
                        let mut _frames: Vec<FrameTimed> =
                            Vec::with_capacity(count as usize);
                        for i in 0..count {
                            _frames[i as usize].time = try!(file.read_f32::<LittleEndian>());
                        }

                        for i in 0..count {
                            _frames[i as usize].frame = try!(Mdl::load_framesingle(header, file));
                        }

                        _frames
                    };

                    Frame::Group(FrameGroup {
                        min: min,
                        max: max,
                        frames: frames,
                    })
                }
            };
            frames.push(frame);
        }
        Ok(frames)
    }

    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, MdlError> {
        let mut mdl_file = try!(File::open(path));

        // TODO: this is atrocious
        let header = Header {
            magic: try!(mdl_file.read_i32::<LittleEndian>()),
            version: try!(mdl_file.read_i32::<LittleEndian>()),
            scale: [try!(mdl_file.read_f32::<LittleEndian>()),
                    try!(mdl_file.read_f32::<LittleEndian>()),
                    try!(mdl_file.read_f32::<LittleEndian>())],
            origin: [try!(mdl_file.read_f32::<LittleEndian>()),
                     try!(mdl_file.read_f32::<LittleEndian>()),
                     try!(mdl_file.read_f32::<LittleEndian>())],
            radius: try!(mdl_file.read_f32::<LittleEndian>()),
            eyes: [try!(mdl_file.read_f32::<LittleEndian>()),
                   try!(mdl_file.read_f32::<LittleEndian>()),
                   try!(mdl_file.read_f32::<LittleEndian>())],
            skin_count: try!(mdl_file.read_i32::<LittleEndian>()),
            skin_w: try!(mdl_file.read_i32::<LittleEndian>()),
            skin_h: try!(mdl_file.read_i32::<LittleEndian>()),
            vertex_count: try!(mdl_file.read_i32::<LittleEndian>()),
            triangle_count: try!(mdl_file.read_i32::<LittleEndian>()),
            frame_count: try!(mdl_file.read_i32::<LittleEndian>()),
            sync_type: try!(mdl_file.read_i32::<LittleEndian>()),
            flags: try!(mdl_file.read_i32::<LittleEndian>()),
            size: try!(mdl_file.read_f32::<LittleEndian>()),
        };

        try!(Mdl::check_header(&header));

        let skins = try!(Mdl::load_skins(&header, &mut mdl_file));
        let texcoords = try!(Mdl::load_texcoords(&header, &mut mdl_file));
        let triangles = try!(Mdl::load_triangles(&header, &mut mdl_file));
        let frames = try!(Mdl::load_frames(&header, &mut mdl_file));

        // check to make sure we loaded the exact size
        match mdl_file.read_u8() {
            Err(e) => {
                match e.kind() {
                    io::ErrorKind::UnexpectedEof => (),
                    _ => return Err(MdlError::Io(e)),
                }
            }

            _ => return Err(MdlError::Other),
        }

        Ok(Mdl {
            header: header,
            skins: skins,
            texcoords: texcoords,
            triangles: triangles,
            frames: frames,
        })
    }
}
