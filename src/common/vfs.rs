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

use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::common::pak::Pak;

use failure::Error;

enum VfsComponent {
    Pak(Pak),
    Directory(PathBuf),
}

pub struct Vfs {
    components: Vec<VfsComponent>,
}

impl Vfs {
    pub fn new() -> Vfs {
        Vfs {
            components: Vec::new(),
        }
    }

    pub fn add_pakfile<P>(&mut self, path: P) -> Result<(), Error>
    where
        P: AsRef<Path>,
    {
        self.components.push(VfsComponent::Pak(Pak::new(path)?));

        Ok(())
    }

    pub fn add_directory<P>(&mut self, path: P) -> Result<(), Error>
    where
        P: AsRef<Path>,
    {
        self.components
            .push(VfsComponent::Directory(path.as_ref().to_path_buf()));

        Ok(())
    }

    pub fn open<S>(&self, virtual_path: S) -> Result<VirtualFile, Error>
    where
        S: AsRef<str>,
    {
        for c in self.components.iter().rev() {
            let vp = virtual_path.as_ref();

            match c {
                VfsComponent::Pak(pak) => {
                    if let Ok(f) = pak.open(vp) {
                        return Ok(VirtualFile::PakBacked(Cursor::new(f)));
                    }
                }

                VfsComponent::Directory(path) => {
                    let mut full_path = path.to_owned();
                    full_path.push(vp);

                    if let Ok(f) = File::open(full_path) {
                        return Ok(VirtualFile::FileBacked(f));
                    }
                }
            }
        }

        bail!("File not found.");
    }
}

pub enum VirtualFile<'a> {
    PakBacked(Cursor<&'a [u8]>),
    FileBacked(File),
}

impl<'a> Read for VirtualFile<'a> {
    fn read(&mut self, buf: &mut [u8]) -> ::std::io::Result<usize> {
        match self {
            VirtualFile::PakBacked(data) => data.read(buf),
            VirtualFile::FileBacked(file) => file.read(buf),
        }
    }
}

impl<'a> Seek for VirtualFile<'a> {
    fn seek(&mut self, pos: SeekFrom) -> ::std::io::Result<u64> {
        match self {
            VirtualFile::PakBacked(data) => data.seek(pos),
            VirtualFile::FileBacked(file) => file.seek(pos),
        }
    }
}
