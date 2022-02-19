use std::error;
use std::fmt::{self, Display, Formatter};
use std::io;
use std::path::PathBuf;

/// Info for a command that exited non 0.
#[derive(Debug, Clone)]
pub struct CommandFailed {
    /// The current working directory of the command ran.
    pub dir: PathBuf,
    /// The command that was ran.
    pub command: PathBuf,
    /// Args passed to the command that was ran.
    pub args: Vec<String>,
    /// The stderr from the command ran.
    pub stderr: Option<String>,
}

impl Display for CommandFailed {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), fmt::Error> {
        write!(
            fmt,
            "command failed: {}: {}",
            self.dir.display(),
            self.command.display()
        )?;
        for arg in &self.args {
            write!(fmt, " {}", arg)?;
        }
        if let Some(stderr) = &self.stderr {
            write!(fmt, ":\n    {}", &stderr.trim().replace('\n', "\n    "))
        } else {
            Ok(())
        }
    }
}

/// The error type for this crate.
#[derive(Debug)]
pub enum Error {
    /// A command exited with non 0.
    CommandFailed(CommandFailed),
    /// An io error occurred.
    Io(io::Error),
}

impl Display for Error {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), fmt::Error> {
        use Error::*;

        match self {
            CommandFailed(e) => e.fmt(fmt),
            Io(e) => e.fmt(fmt),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        use Error::*;

        match self {
            Io(e) => e.source(),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}
