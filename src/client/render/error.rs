use crate::common::{vfs::VfsError, wad::WadError};
use failure::{Backtrace, Context, Fail};
use std::{
    convert::From,
    fmt::{self, Display},
};

#[derive(Debug)]
pub struct RenderError {
    inner: Context<RenderErrorKind>,
}

impl RenderError {
    pub fn kind(&self) -> RenderErrorKind {
        *self.inner.get_context()
    }
}

impl From<RenderErrorKind> for RenderError {
    fn from(kind: RenderErrorKind) -> Self {
        RenderError {
            inner: Context::new(kind),
        }
    }
}

impl From<VfsError> for RenderError {
    fn from(vfs_error: VfsError) -> Self {
        match vfs_error {
            VfsError::NoSuchFile(_) => vfs_error.context(RenderErrorKind::ResourceNotLoaded).into(),
            _ => vfs_error.context(RenderErrorKind::Other).into(),
        }
    }
}

impl From<WadError> for RenderError {
    fn from(wad_error: WadError) -> Self {
        wad_error.context(RenderErrorKind::ResourceNotLoaded).into()
    }
}

impl From<Context<RenderErrorKind>> for RenderError {
    fn from(inner: Context<RenderErrorKind>) -> Self {
        RenderError { inner }
    }
}

impl Fail for RenderError {
    fn cause(&self) -> Option<&dyn Fail> {
        self.inner.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

impl Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Display::fmt(&self.inner, f)
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug, Fail)]
pub enum RenderErrorKind {
    #[fail(display = "Failed to load resource")]
    ResourceNotLoaded,
    #[fail(display = "Unspecified render error")]
    Other,
}
