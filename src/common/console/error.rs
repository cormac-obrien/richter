use failure::{Backtrace, Context, Fail};
use std::{
    convert::From,
    fmt::{self, Display},
};

#[derive(Debug)]
pub struct ConsoleError {
    inner: Context<ConsoleErrorKind>,
}

impl ConsoleError {
    pub fn kind(&self) -> ConsoleErrorKind {
        self.inner.get_context().clone()
    }
}

impl From<ConsoleErrorKind> for ConsoleError {
    fn from(kind: ConsoleErrorKind) -> Self {
        ConsoleError {
            inner: Context::new(kind),
        }
    }
}

impl From<Context<ConsoleErrorKind>> for ConsoleError {
    fn from(inner: Context<ConsoleErrorKind>) -> Self {
        ConsoleError { inner }
    }
}

impl Fail for ConsoleError {
    fn cause(&self) -> Option<&Fail> {
        self.inner.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

impl Display for ConsoleError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Display::fmt(&self.inner, f)
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Fail)]
pub enum ConsoleErrorKind {
    #[fail(display = "Failed to parse cvar as a number: {} = {}", name, value)]
    CvarParseFailed { name: String, value: String },
    #[fail(display = "Command already registered: {}", name)]
    DuplicateCommand { name: String },
    #[fail(display = "Cvar already registered: {}", name)]
    DuplicateCvar { name: String },
    #[fail(display = "No such command: {}", name)]
    NoSuchCommand { name: String },
    #[fail(display = "No such cvar: {}", name)]
    NoSuchCvar { name: String },
}
