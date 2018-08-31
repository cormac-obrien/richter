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

use std::collections::HashMap;
use std::io::BufReader;
use std::io::Cursor;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;

use common::util;

use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use failure::Error;

// see definition of lumpinfo_t:
// https://github.com/id-Software/Quake/blob/master/WinQuake/wad.h#L54-L63
const LUMPINFO_SIZE: usize = 32;
const MAGIC: u32 = 'W' as u32 | ('A' as u32) << 8 | ('D' as u32) << 16 | ('2' as u32) << 24;

pub struct QPic {
    width: u32,
    height: u32,
    indices: Box<[u8]>,
}

impl QPic {
    pub fn load<R>(data: R) -> Result<QPic, Error> where R: Read + Seek {
        let mut reader = BufReader::new(data);

        let width = reader.read_u32::<LittleEndian>()?;
        let height = reader.read_u32::<LittleEndian>()?;

        let mut indices = Vec::new();
        (&mut reader).take((width * height) as u64).read_to_end(&mut indices)?;

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
    pub fn load<R>(data: R) -> Result <Wad, Error> where R: Read + Seek {
        let mut reader = BufReader::new(data);

        let magic = reader.read_u32::<LittleEndian>()?;
        ensure!(magic == MAGIC, "Bad magic number for WAD: got {}, should be {}", magic, MAGIC);

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

            lump_infos.push(LumpInfo {
                offset,
                size,
                name
            });
        }

        let mut files = HashMap::new();

        for lump_info in lump_infos {
            let mut data = Vec::with_capacity(lump_info.size as usize);
            reader.seek(SeekFrom::Start(lump_info.offset as u64))?;
            (&mut reader).take(lump_info.size as u64).read_to_end(&mut data)?;
            files.insert(lump_info.name.to_owned(), data.into_boxed_slice());
        }

        Ok(Wad { files })
    }

    pub fn open_conchars(&self) -> Result<QPic, Error> {
        match self.files.get("CONCHARS") {
            Some(ref data) => {
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

    pub fn open_qpic<S>(&self, name: S) -> Result<QPic, Error> where S: AsRef<str> {
        if name.as_ref() == "CONCHARS" {
            bail!("conchars must be opened with open_conchars()");
        }

        match self.files.get(name.as_ref()) {
            Some(ref data) => QPic::load(Cursor::new(data)),
            None => bail!("File not found in WAD: {}", name.as_ref()),
        }
    }
}

