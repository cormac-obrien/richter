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

//! Quake PAK archive manipulation.

use std::{
    collections::{hash_map::Iter, HashMap},
    fs,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};

use byteorder::{LittleEndian, ReadBytesExt};
use thiserror::Error;

const PAK_MAGIC: [u8; 4] = [b'P', b'A', b'C', b'K'];
const PAK_ENTRY_SIZE: usize = 64;

#[derive(Error, Debug)]
pub enum PakError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("Invalid magic number: {0:?}")]
    InvalidMagicNumber([u8; 4]),
    #[error("Invalid file table offset: {0}")]
    InvalidTableOffset(i32),
    #[error("Invalid file table size: {0}")]
    InvalidTableSize(i32),
    #[error("Invalid file offset: {0}")]
    InvalidFileOffset(i32),
    #[error("Invalid file size: {0}")]
    InvalidFileSize(i32),
    #[error("File name too long: {0}")]
    FileNameTooLong(String),
    #[error("Non-UTF-8 file name: {0}")]
    NonUtf8FileName(#[from] std::string::FromUtf8Error),
    #[error("No such file in PAK archive: {0}")]
    NoSuchFile(String),
}

/// An open Pak archive.
#[derive(Debug)]
pub struct Pak(HashMap<String, Box<[u8]>>);

impl Pak {
    // TODO: rename to from_path or similar
    pub fn new<P>(path: P) -> Result<Pak, PakError>
    where
        P: AsRef<Path>,
    {
        debug!("Opening {}", path.as_ref().to_str().unwrap());

        let mut infile = fs::File::open(path)?;
        let mut magic = [0u8; 4];
        infile.read_exact(&mut magic)?;

        if magic != PAK_MAGIC {
            Err(PakError::InvalidMagicNumber(magic))?;
        }

        // Locate the file table
        let table_offset = match infile.read_i32::<LittleEndian>()? {
            o if o <= 0 => Err(PakError::InvalidTableOffset(o))?,
            o => o as u32,
        };

        let table_size = match infile.read_i32::<LittleEndian>()? {
            s if s <= 0 || s as usize % PAK_ENTRY_SIZE != 0 => Err(PakError::InvalidTableSize(s))?,
            s => s as u32,
        };

        let mut map = HashMap::new();

        for i in 0..(table_size as usize / PAK_ENTRY_SIZE) {
            let entry_offset = table_offset as u64 + (i * PAK_ENTRY_SIZE) as u64;
            infile.seek(SeekFrom::Start(entry_offset))?;

            let mut path_bytes = [0u8; 56];
            infile.read_exact(&mut path_bytes)?;

            let file_offset = match infile.read_i32::<LittleEndian>()? {
                o if o <= 0 => Err(PakError::InvalidFileOffset(o))?,
                o => o as u32,
            };

            let file_size = match infile.read_i32::<LittleEndian>()? {
                s if s <= 0 => Err(PakError::InvalidFileSize(s))?,
                s => s as u32,
            };

            let last = path_bytes
                .iter()
                .position(|b| *b == 0)
                .ok_or(PakError::FileNameTooLong(
                    String::from_utf8_lossy(&path_bytes).into_owned(),
                ))?;
            let path = String::from_utf8(path_bytes[0..last].to_vec())?;
            infile.seek(SeekFrom::Start(file_offset as u64))?;

            let mut data: Vec<u8> = Vec::with_capacity(file_size as usize);
            (&mut infile)
                .take(file_size as u64)
                .read_to_end(&mut data)?;

            map.insert(path, data.into_boxed_slice());
        }

        Ok(Pak(map))
    }

    /// Opens a file in the file tree for reading.
    ///
    /// # Examples
    /// ```no_run
    /// # extern crate richter;
    /// use richter::common::pak::Pak;
    ///
    /// # fn main() {
    /// let mut pak = Pak::new("pak0.pak").unwrap();
    /// let progs_dat = pak.open("progs.dat").unwrap();
    /// # }
    /// ```
    pub fn open<S>(&self, path: S) -> Result<&[u8], PakError>
    where
        S: AsRef<str>,
    {
        let path = path.as_ref();
        self.0
            .get(path)
            .map(|s| s.as_ref())
            .ok_or(PakError::NoSuchFile(path.to_owned()))
    }

    pub fn iter<'a>(&self) -> Iter<String, impl AsRef<[u8]>> {
        self.0.iter()
    }
}
