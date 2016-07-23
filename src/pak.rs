// Copyright © 2016 Cormac O'Brien
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

use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::Path;
use std::string;
use byteorder::{LittleEndian, ReadBytesExt};

const PAK_MAGIC: [u8; 4] = [b'P', b'A', b'C', b'K'];
const PAK_ENTRY_SIZE: usize = 64;

struct Header {
    // Should equal PAK_MAGIC, the ASCII string 'PACK'
    pub magic: [u8; 4],

    // Offset in bytes of the file table
    pub offset: u32,

    // Size in bytes of the file table. This will be 64 bytes * the number of files.
    pub size: u32,
}

struct Entry {
    // The virtual path to the file.
    pub path: [u8; 56],

    // The actual position of the file data.
    pub offset: u32,

    // The size of the file data.
    pub size: u32,
}

#[derive(Debug)]
pub enum PakError {
    Io(io::Error),
    Utf8(string::FromUtf8Error),
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
            PakError::Io(_) => "I/O error",
            PakError::Utf8(_) => "Utf-8 decoding error",
            PakError::Other => "Unknown error",
        }
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            PakError::Io(ref i) => Some(i),
            PakError::Utf8(ref i) => Some(i),
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

struct FileData {
    name: String,
    data: Vec<u8>,
}

// TODO: make this a HashMap or similar
pub struct Pak {
    children: Vec<FileData>,
}

impl Pak {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, PakError> {
        println!("Opening {}", path.as_ref().to_str().unwrap());
        let mut infile = try!(fs::File::open(path).map_err(PakError::Io));

        let mut header = Header {
            magic: [0; 4],
            offset: 0,
            size: 0,
        };

        try!(infile.read(&mut header.magic).map_err(PakError::Io));

        if header.magic != PAK_MAGIC {
            return Err(PakError::Other);
        }

        header.offset = {
            let _offset = try!(infile.read_i32::<LittleEndian>().map_err(PakError::Io));
            if _offset <= 0 {
                return Err(PakError::Other);
            }
            _offset as u32
        };

        header.size = {
            let _size = try!(infile.read_i32::<LittleEndian>().map_err(PakError::Io));
            if _size <= 0 {
                return Err(PakError::Other);
            }
            _size as u32
        };

        let mut result: Pak = Pak { children: Vec::new() };

        // Create a pak::FileData for each entry
        for i in 0..(header.size as usize / PAK_ENTRY_SIZE) {
            let mut entry = Entry {
                path: [0; 56],
                offset: 0,
                size: 0,
            };

            let entry_offset = header.offset as u64 + (i * PAK_ENTRY_SIZE) as u64;
            try!(infile.seek(SeekFrom::Start(entry_offset)).map_err(PakError::Io));
            try!(infile.read(&mut entry.path).map_err(PakError::Io));

            entry.offset = {
                let _offset = try!(infile.read_i32::<LittleEndian>().map_err(PakError::Io));
                if _offset <= 0 {
                    return Err(PakError::Other);
                }
                _offset as u32
            };

            entry.size = {
                let _size = try!(infile.read_i32::<LittleEndian>().map_err(PakError::Io));
                if _size <= 0 {
                    return Err(PakError::Other);
                }
                _size as u32
            };

            let last = {
                let mut _last: usize = 0;
                while entry.path[_last] != 0 {
                    _last += 1;
                }
                _last
            };

            let path = try!(String::from_utf8(entry.path[0..last].to_vec())
                                .map_err(PakError::Utf8));

            try!(infile.seek(SeekFrom::Start(entry.offset as u64)).map_err(PakError::Io));

            let mut f: FileData = FileData {
                name: path.clone(),
                data: Vec::with_capacity(entry.size as usize),
            };

            let mut indata = Read::by_ref(&mut infile).take(entry.size as u64);
            try!(indata.read_to_end(&mut f.data).map_err(PakError::Io));

            result.children.push(f);
        }
        result.children.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(result)
    }

    pub fn open(&self, path: &str) -> Option<File> {
        match self.children.binary_search_by(|a| a.name.as_str().cmp(path)) {
            Err(_) => None,
            Ok(i) => Some(File::new(&self.children[i])),
        }
    }
}

pub struct File<'f> {
    cursor: Cursor<&'f Vec<u8>>,
}

impl<'f> File<'f> {
    fn new(fd: &'f FileData) -> File {
        File { cursor: Cursor::new(&fd.data) }
    }
}

impl<'f> Read for File<'f> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.cursor.read(buf)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
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

        let setup_status = Command::new("sh")
                                   .arg(setup_path)
                                   .status()
                                   .expect("Setup failed.");

        if !setup_status.success() {
            panic!("Setup script failed.");
        }
    }

    fn teardown() {
    }

    #[test]
    fn test_pak0() {
        setup();

        let pak0_path = {
            let mut _pak0_path = RESOURCE_DIR.to_owned();
            _pak0_path.push("pak0.pak");
            _pak0_path
        };

        let pak0 = Pak::load(pak0_path).expect("pak0 load failed!");

        teardown();
    }
}
