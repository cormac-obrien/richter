// Copyright © 2018 Cormac O'Brien
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use std::{
    collections::HashMap,
    convert::From,
    fmt::{self, Display},
    io::{self, BufReader, Cursor, Read, Seek, SeekFrom},
};

use crate::common::util;

use byteorder::{LittleEndian, ReadBytesExt};
use failure::{Backtrace, Context, Error, Fail};

// see definition of lumpinfo_t:
// https://github.com/id-Software/Quake/blob/master/WinQuake/wad.h#L54-L63
const LUMPINFO_SIZE: usize = 32;
const MAGIC: u32 = 'W' as u32 | ('A' as u32) << 8 | ('D' as u32) << 16 | ('2' as u32) << 24;

#[derive(Debug)]
pub struct WadError {
    inner: Context<WadErrorKind>,
}

impl WadError {
    pub fn kind(&self) -> WadErrorKind {
        *self.inner.get_context()
    }
}

impl From<WadErrorKind> for WadError {
    fn from(kind: WadErrorKind) -> Self {
        WadError {
            inner: Context::new(kind),
        }
    }
}

impl From<Context<WadErrorKind>> for WadError {
    fn from(inner: Context<WadErrorKind>) -> Self {
        WadError { inner }
    }
}

impl From<io::Error> for WadError {
    fn from(io_error: io::Error) -> Self {
        let kind = io_error.kind();
        match kind {
            io::ErrorKind::UnexpectedEof => io_error.context(WadErrorKind::UnexpectedEof).into(),
            _ => io_error.context(WadErrorKind::Io).into(),
        }
    }
}

impl Fail for WadError {
    fn cause(&self) -> Option<&dyn Fail> {
        self.inner.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

impl Display for WadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Display::fmt(&self.inner, f)
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug, Fail)]
pub enum WadErrorKind {
    #[fail(display = "CONCHARS must be loaded with the dedicated function")]
    ConcharsUseDedicatedFunction,
    #[fail(display = "Invalid magic number")]
    InvalidMagicNumber,
    #[fail(display = "I/O error")]
    Io,
    #[fail(display = "No such file in WAD")]
    NoSuchFile,
    #[fail(display = "Failed to load QPic")]
    QPicNotLoaded,
    #[fail(display = "Unexpected end of data")]
    UnexpectedEof,
}

pub struct QPic {
    width: u32,
    height: u32,
    indices: Box<[u8]>,
}

impl QPic {
    pub fn load<R>(data: R) -> Result<QPic, WadError>
    where
        R: Read + Seek,
    {
        let mut reader = BufReader::new(data);

        let width = reader.read_u32::<LittleEndian>()?;
        let height = reader.read_u32::<LittleEndian>()?;

        let mut indices = Vec::new();
        (&mut reader)
            .take((width * height) as u64)
            .read_to_end(&mut indices)?;

        Ok(QPic {
            width,
            height,
            indices: indices.into_boxed_slice(),
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn indices(&self) -> &[u8] {
        &self.indices
    }
}

struct LumpInfo {
    offset: u32,
    size: u32,
    name: String,
}

pub struct Wad {
    files: HashMap<String, Box<[u8]>>,
}

impl Wad {
    pub fn load<R>(data: R) -> Result<Wad, Error>
    where
        R: Read + Seek,
    {
        let mut reader = BufReader::new(data);

        let magic = reader.read_u32::<LittleEndian>()?;
        if magic != MAGIC {
            return Err(WadErrorKind::InvalidMagicNumber.into());
        }

        let lump_count = reader.read_u32::<LittleEndian>()?;
        let lumpinfo_ofs = reader.read_u32::<LittleEndian>()?;

        reader.seek(SeekFrom::Start(lumpinfo_ofs as u64))?;

        let mut lump_infos = Vec::new();

        for _ in 0..lump_count {
            // TODO sanity check these values
            let offset = reader.read_u32::<LittleEndian>()?;
            let _size_on_disk = reader.read_u32::<LittleEndian>()?;
            let size = reader.read_u32::<LittleEndian>()?;
            let _type = reader.read_u8()?;
            let _compression = reader.read_u8()?;
            let _pad = reader.read_u16::<LittleEndian>()?;
            let mut name_bytes = [0u8; 16];
            reader.read_exact(&mut name_bytes)?;
            let name_lossy = String::from_utf8_lossy(&name_bytes);
            debug!("name: {}", name_lossy);
            let name = util::read_cstring(&mut BufReader::new(Cursor::new(name_bytes)))?;

            lump_infos.push(LumpInfo { offset, size, name });
        }

        let mut files = HashMap::new();

        for lump_info in lump_infos {
            let mut data = Vec::with_capacity(lump_info.size as usize);
            reader.seek(SeekFrom::Start(lump_info.offset as u64))?;
            (&mut reader)
                .take(lump_info.size as u64)
                .read_to_end(&mut data)?;
            files.insert(lump_info.name.to_owned(), data.into_boxed_slice());
        }

        Ok(Wad { files })
    }

    pub fn open_conchars(&self) -> Result<QPic, Error> {
        match self.files.get("CONCHARS") {
            Some(data) => {
                let width = 128;
                let height = 128;
                let indices = Vec::from(&data[..(width * height) as usize]);

                Ok(QPic {
                    width,
                    height,
                    indices: indices.into_boxed_slice(),
                })
            }

            None => bail!("conchars not found in WAD"),
        }
    }

    pub fn open_qpic<S>(&self, name: S) -> Result<QPic, WadError>
    where
        S: AsRef<str>,
    {
        if name.as_ref() == "CONCHARS" {
            return Err(WadErrorKind::ConcharsUseDedicatedFunction.into());
        }

        match self.files.get(name.as_ref()) {
            Some(ref data) => QPic::load(Cursor::new(data)),
            None => Err(WadErrorKind::NoSuchFile.into()),
        }
    }
}
