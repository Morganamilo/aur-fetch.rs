use std::error;
use std::fmt::{self, Display, Formatter};
use std::io;

/// The error type for this crate.
#[derive(Debug)]
pub enum Error {
    /// A command failed to run. Stderr is captured.
    CommandFailed(String),
    /// An io error occurred.
    Io(io::Error),
}

impl Display for Error {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), fmt::Error> {
        use Error::*;

        match self {
            CommandFailed(e) => write!(fmt, "command failed: {}", e.trim()),
            Io(e) => e.fmt(fmt),
        }
    }
}

impl error::Error for Error {
    fn cause(&self) -> Option<&dyn error::Error> {
        use Error::*;

        match self {
            Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}
