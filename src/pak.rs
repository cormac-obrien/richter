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

//! Quake PAK archive manipulation.

extern crate byteorder;

use std::collections::hash_map::Iter;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::string;
use byteorder::{LittleEndian, ReadBytesExt};

const PAK_MAGIC: [u8; 4] = [b'P', b'A', b'C', b'K'];
const PAK_ENTRY_SIZE: usize = 64;

#[derive(Debug)]
pub enum PakError {
    Io(io::Error),
    Utf8(string::FromUtf8Error),
    Invalid,
    Other,
}

impl fmt::Display for PakError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

impl Error for PakError {
    fn description(&self) -> &str {
        match *self {
            PakError::Io(ref e) => e.description(),
            PakError::Utf8(ref e) => e.description(),
            PakError::Invalid => "Not a valid PAK file",
            PakError::Other => "Unknown error",
        }
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            PakError::Io(ref i) => Some(i),
            PakError::Utf8(ref i) => Some(i),
            PakError::Invalid => None,
            PakError::Other => None,
        }
    }
}

impl From<io::Error> for PakError {
    fn from(err: io::Error) -> PakError {
        PakError::Io(err)
    }
}

impl From<string::FromUtf8Error> for PakError {
    fn from(err: string::FromUtf8Error) -> PakError {
        PakError::Utf8(err)
    }
}

/// A virtual file tree loaded from a PAK archive.
pub struct Pak(HashMap<String, Box<[u8]>>);

impl Pak {
    pub fn new() -> Pak {
        Pak(HashMap::new())
    }

    /// Attempts to load a virtual file tree from a PAK archive.
    ///
    /// # Examples
    /// ```
    /// use pak::Pak;
    ///
    /// let pak0 = Pak::load("pak0.pak");
    /// ```
    pub fn add<P: AsRef<Path>>(&mut self, path: P) -> Result<(), PakError> {
        debug!("Opening {}", path.as_ref().to_str().unwrap());

        let mut infile = try!(fs::File::open(path));

        let mut magic = [0u8; 4];
        try!(infile.read(&mut magic));

        if magic != PAK_MAGIC {
            return Err(PakError::Invalid);
        }

        // Locate the file table

        let wad_offset = match try!(infile.read_i32::<LittleEndian>()) {
            o if o <= 0 => return Err(PakError::Invalid),
            o => o as u32,
        };

        let wad_size = match try!(infile.read_i32::<LittleEndian>()) {
            s if s <= 0 => return Err(PakError::Invalid),
            s => s as u32,
        };

        for i in 0..(wad_size as usize / PAK_ENTRY_SIZE) {
            let entry_offset = wad_offset as u64 + (i * PAK_ENTRY_SIZE) as u64;
            try!(infile.seek(SeekFrom::Start(entry_offset)));

            let mut path_bytes = [0u8; 56];
            try!(infile.read(&mut path_bytes));

            let file_offset = match try!(infile.read_i32::<LittleEndian>()) {
                o if o <= 0 => return Err(PakError::Invalid),
                o => o as u32,
            };

            let file_size = match try!(infile.read_i32::<LittleEndian>()) {
                s if s <= 0 => return Err(PakError::Invalid),
                s => s as u32,
            };

            let last = {
                let mut _last: usize = 0;
                while path_bytes[_last] != 0 {
                    _last += 1;
                }
                _last
            };

            let path = try!(String::from_utf8(path_bytes[0..last].to_vec()));

            try!(infile.seek(SeekFrom::Start(file_offset as u64)));

            let mut data: Vec<u8> = Vec::with_capacity(file_size as usize);
            try!((&mut infile).take(file_size as u64).read_to_end(&mut data));

            self.0.insert(path, data.into_boxed_slice());
        }
        Ok(())
    }

    /// Opens a file in the file tree for reading.
    ///
    /// # Examples
    /// ```
    /// use pak::Pak;
    ///
    /// let pak0 = Pak::load("pak0.pak");
    /// let progs_dat = pak0.open("progs.dat");
    /// ```
    pub fn open(&self, path: &str) -> Option<&[u8]> {
        match self.0.get(path) {
            Some(data) => Some(&data),
            None => None,
        }
    }

    pub fn iter<'a>(&self) -> Iter<String, Box<[u8]>> {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::process::Command;

    use pak::Pak;

    lazy_static! {
        static ref RESOURCE_DIR: PathBuf = {
            let mut _res = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            _res.push("res");
            _res.push("test");
            _res
        };
    }

    fn setup() {
        let setup_path = {
            let mut _setup_path = RESOURCE_DIR.to_owned();
            _setup_path.push("setup.sh");
            _setup_path
        };

        let setup_status = Command::new("sh").arg(setup_path).status().expect(
            "Setup failed.",
        );

        if !setup_status.success() {
            panic!("Setup script failed.");
        }
    }

    fn teardown() {}

    #[test]
    fn test_pak0() {
        setup();

        let pak0_path = {
            let mut _pak0_path = RESOURCE_DIR.to_owned();
            _pak0_path.push("pak0.pak");
            _pak0_path
        };

        let mut pak0 = Pak::new();
        pak0.add(pak0_path).expect("pak0 load failed!");
        assert!(pak0.open("progs.dat").is_some());

        teardown();
    }
}
