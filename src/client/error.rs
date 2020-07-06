use failure::{Backtrace, Context, Fail};
use std::{
    convert::From,
    fmt::{self, Display},
};

#[derive(Debug)]
pub struct ClientError {
    inner: Context<ClientErrorKind>,
}

impl ClientError {
    pub fn kind(&self) -> ClientErrorKind {
        self.inner.get_context().clone()
    }
}

impl From<ClientErrorKind> for ClientError {
    fn from(kind: ClientErrorKind) -> Self {
        ClientError {
            inner: Context::new(kind),
        }
    }
}

impl From<Context<ClientErrorKind>> for ClientError {
    fn from(inner: Context<ClientErrorKind>) -> Self {
        ClientError { inner }
    }
}

impl Fail for ClientError {
    fn cause(&self) -> Option<&dyn Fail> {
        self.inner.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

impl Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Display::fmt(&self.inner, f)
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Fail)]
pub enum ClientErrorKind {
    #[fail(display = "Connection rejected: \"{}\"", message)]
    ConnectionRejected { message: String },
    #[fail(display = "Couldn't read cvar value: \"{}\"", name)]
    Cvar { name: String },
    #[fail(display = "Server sent an invalid port number ({})", port)]
    InvalidConnectPort { port: i32 },
    #[fail(display = "Server sent an inappropriate connect response")]
    InvalidConnectResponse,
    #[fail(display = "Invalid server address")]
    InvalidServerAddress,
    #[fail(display = "No response from server")]
    NoResponse,
    #[fail(display = "No client with ID {}", id)]
    NoSuchClient { id: usize },
    #[fail(display = "No player with ID {}", id)]
    NoSuchPlayer { id: usize },
    #[fail(display = "Failed to load resource: {}", name)]
    ResourceNotLoaded { name: String },
}
