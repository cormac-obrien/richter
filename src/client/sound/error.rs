use failure::{Backtrace, Context, Fail};
use std::{
    convert::From,
    fmt::{self, Display},
};

#[derive(Debug)]
pub struct SoundError {
    inner: Context<SoundErrorKind>,
}

impl SoundError {
    pub fn kind(&self) -> SoundErrorKind {
        self.inner.get_context().clone()
    }
}

impl From<SoundErrorKind> for SoundError {
    fn from(kind: SoundErrorKind) -> Self {
        SoundError {
            inner: Context::new(kind),
        }
    }
}

impl From<Context<SoundErrorKind>> for SoundError {
    fn from(inner: Context<SoundErrorKind>) -> Self {
        SoundError { inner }
    }
}

impl Fail for SoundError {
    fn cause(&self) -> Option<&dyn Fail> {
        self.inner.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

impl Display for SoundError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Display::fmt(&self.inner, f)
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Fail)]
pub enum SoundErrorKind {
    #[fail(display = "Failed to decode audio: {}", name)]
    DecodeFailed { name: String },
    #[fail(display = "I/O error reading file: {}", name)]
    Io { name: String },
    #[fail(display = "Failed to read WAV data: {}", name)]
    WavReadFailed { name: String },
    #[fail(display = "Failed to write WAV data: {}", name)]
    WavWriteFailed { name: String },
}
