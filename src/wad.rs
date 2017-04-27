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

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::io::{Cursor, Error as IoError, Read, Seek, SeekFrom};

use load::{Load, LoadError};
use num::FromPrimitive;

const MAGIC: [u8; 4] = [b'W', b'A', b'D', b'2'];

#[derive(Debug)]
pub enum WadError {
    Io(IoError),
    Invalid,
    Load(LoadError),
}

impl fmt::Display for WadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            WadError::Io(ref err) => write!(f, "I/O error: {}", err),
            WadError::Invalid => write!(f, "Invalid WAD file"),
            WadError::Load(ref err) => write!(f, "Load error: {}", err),
        }
    }
}

impl Error for WadError {
    fn description(&self) -> &str {
        match *self {
            WadError::Io(ref err) => err.description(),
            WadError::Invalid => "Invalid WAD file",
            WadError::Load(ref err) => err.description(),
        }
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            WadError::Io(ref err) => Some(err),
            WadError::Invalid => None,
            WadError::Load(ref err) => Some(err),
        }
    }
}

impl From<IoError> for WadError {
    fn from(err: IoError) -> Self {
        WadError::Io(err)
    }
}

impl From<LoadError> for WadError {
    fn from(err: LoadError) -> Self {
        WadError::Load(err)
    }
}

pub struct Wad {
    entries: HashMap<String, Box<[u8]>>,
}

struct WadEntry {
    offset: usize,
    disk_size: usize,
    mem_size: usize,
    kind: WadEntryKind,
    cmpr: u8,
    name: String,
}

#[derive(FromPrimitive)]
enum WadEntryKind {
    Palette = 0x40,
    Status = 0x42,
    Texture = 0x44,
    Console = 0x45,
}

#[derive(FromPrimitive)]
enum WadCmprKind {
    None = 0,
    Lzss = 1,
}

impl Wad {
    pub fn load(data: &[u8]) -> Result<Wad, WadError> {
        let mut curs = Cursor::new(data);

        // verify magic number
        let mut magic = [0u8; 4];
        curs.read(&mut magic)?;
        if magic != MAGIC {
            return Err(WadError::Invalid);
        }

        // find directory
        let entry_count = curs.load_i32le(Some(&(0..)))? as usize;
        let dir_offset = curs.load_i32le(Some(&(0..)))? as u64;
        curs.seek(SeekFrom::Start(dir_offset))?;

        let mut entries = Vec::new();
        for _ in 0..entry_count {
            let offset = curs.load_i32le(Some(&(0..)))? as usize;
            let disk_size = curs.load_i32le(Some(&(0..)))? as usize;
            let mem_size = curs.load_i32le(Some(&(0..)))? as usize;
            let kind = curs.load_u8(None)?;
            let cmpr = curs.load_u8(None)?;
            let _ = curs.load_u16le(None)?; // skip padding
            let mut name = [0u8; 16];
            curs.read(&mut name)?;
            let mut name_len = 0;
            while name[name_len] != b'\0' {
                name_len += 1;
            }
            entries.push(WadEntry {
                offset: offset,
                disk_size: disk_size,
                mem_size: mem_size,
                kind: WadEntryKind::from_u8(kind).unwrap(),
                cmpr: cmpr,
                name: String::from_utf8(Vec::from(&name[..name_len])).unwrap(),
            });
        }

        let mut map = HashMap::new();
        for entry in entries {
            // TODO: need an LZSS decompresser
            if entry.cmpr != 0 {
                println!("WAD contains LZSS-compressed data");
                unimplemented!();
            }

            curs.seek(SeekFrom::Start(entry.offset as u64))?;
            let mut bytes = Vec::new();
            (&mut curs).take(entry.disk_size as u64).read_to_end(&mut bytes)?;
            map.insert(entry.name, bytes.into_boxed_slice());
        }

        Ok(Wad {
            entries: map,
        })
    }
}
