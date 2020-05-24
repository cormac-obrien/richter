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

use std::{
    convert::From,
    fmt::{self, Display},
    fs::File,
    io::{Cursor, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use crate::common::pak::Pak;

use failure::{Backtrace, Context, Error, Fail, ResultExt};

#[derive(Debug)]
pub struct VfsError {
    inner: Context<VfsErrorKind>,
}

impl VfsError {
    pub fn kind(&self) -> VfsErrorKind {
        self.inner.get_context().clone()
    }
}

impl From<VfsErrorKind> for VfsError {
    fn from(kind: VfsErrorKind) -> Self {
        VfsError {
            inner: Context::new(kind),
        }
    }
}

impl From<Context<VfsErrorKind>> for VfsError {
    fn from(inner: Context<VfsErrorKind>) -> Self {
        VfsError { inner }
    }
}

impl Fail for VfsError {
    fn cause(&self) -> Option<&dyn Fail> {
        self.inner.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

impl Display for VfsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Display::fmt(&self.inner, f)
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Fail)]
pub enum VfsErrorKind {
    #[fail(display = "Couldn't load pakfile: {}", path)]
    PakfileNotLoaded { path: String },
    #[fail(display = "File does not exist: {}", path)]
    NoSuchFile { path: String },
}

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

    pub fn add_pakfile<P>(&mut self, path: P) -> Result<(), VfsError>
    where
        P: AsRef<Path>,
    {
        let path = path.as_ref();

        self.components
            .push(VfsComponent::Pak(Pak::new(path).context(
                VfsErrorKind::PakfileNotLoaded {
                    path: path.to_string_lossy().into_owned(),
                },
            )?));

        Ok(())
    }

    pub fn add_directory<P>(&mut self, path: P) -> Result<(), VfsError>
    where
        P: AsRef<Path>,
    {
        self.components
            .push(VfsComponent::Directory(path.as_ref().to_path_buf()));

        Ok(())
    }

    pub fn open<S>(&self, virtual_path: S) -> Result<VirtualFile, VfsError>
    where
        S: AsRef<str>,
    {
        let vp = virtual_path.as_ref();

        // iterate in reverse so later PAKs overwrite earlier ones
        for c in self.components.iter().rev() {

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

        Err(VfsErrorKind::NoSuchFile { path: vp.to_owned() })?
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
