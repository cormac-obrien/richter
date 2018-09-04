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

use std::collections::hash_map::Iter;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use byteorder::{LittleEndian, ReadBytesExt};
use failure::Error;

const PAK_MAGIC: [u8; 4] = [b'P', b'A', b'C', b'K'];
const PAK_ENTRY_SIZE: usize = 64;

pub struct Pak(HashMap<String, Box<[u8]>>);

impl Pak {
    pub fn new<P>(path: P) -> Result<Pak, Error>
    where
        P: AsRef<Path>,
    {
        debug!("Opening {}", path.as_ref().to_str().unwrap());

        let mut infile = try!(fs::File::open(path));

        let mut magic = [0u8; 4];
        try!(infile.read(&mut magic));

        ensure!(magic == PAK_MAGIC, "Invalid magic number");

        // Locate the file table

        let wad_offset = match try!(infile.read_i32::<LittleEndian>()) {
            o if o <= 0 => bail!("Negative file table offset"),
            o => o as u32,
        };

        let wad_size = match try!(infile.read_i32::<LittleEndian>()) {
            s if s <= 0 => bail!("Negative file table size"),
            s => s as u32,
        };

        let mut map = HashMap::new();

        for i in 0..(wad_size as usize / PAK_ENTRY_SIZE) {
            let entry_offset = wad_offset as u64 + (i * PAK_ENTRY_SIZE) as u64;
            infile.seek(SeekFrom::Start(entry_offset))?;

            let mut path_bytes = [0u8; 56];
            infile.read(&mut path_bytes)?;

            let file_offset = match infile.read_i32::<LittleEndian>()? {
                o if o <= 0 => bail!("Negative file offset"),
                o => o as u32,
            };

            let file_size = match infile.read_i32::<LittleEndian>()? {
                s if s <= 0 => bail!("Negative file size"),
                s => s as u32,
            };

            let last = {
                let mut _last: usize = 0;
                while path_bytes[_last] != 0 {
                    _last += 1;
                }
                _last
            };

            let path = String::from_utf8(path_bytes[0..last].to_vec())?;

            infile.seek(SeekFrom::Start(file_offset as u64))?;

            let mut data: Vec<u8> = Vec::with_capacity(file_size as usize);
            (&mut infile).take(file_size as u64).read_to_end(&mut data)?;

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
    pub fn open<S>(&self, path: S) -> Result<&[u8], Error>
    where
        S: AsRef<str>,
    {
        match self.0.get(path.as_ref()) {
            Some(data) => Ok(&data),
            None => bail!("No \"{}\" in pakfile", path.as_ref()),
        }
    }

    pub fn iter<'a>(&self) -> Iter<String, impl AsRef<[u8]>> {
        self.0.iter()
    }
}
