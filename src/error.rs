use std::fmt;
use std::io;

#[derive(Debug)]
pub enum Error {
    Usage,
    Nix(nix::Error),
    Io(io::Error),
    CommandNotFound(String),
}

// implementing the fmt::Display trait for the Error type
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Usage => write!(f, "usage: rtrace <command> [args...]"),
            Error::Nix(errno) => write!(f, "system call failed: {errno}"),
            Error::Io(err) => write!(f, "i/o error: {err}"),
            Error::CommandNotFound(cmd) => write!(f, "command not found: {cmd}"),
        }
    }
}

// marks the type as a standard error trait it is empty cuz the required fn are Debug, Display 
// that are already implemented

impl std::error::Error for Error {} 
impl From<nix::Error> for Error {
    fn from(errno: nix::Error) -> Self {
        Error::Nix(errno)
    }
}
impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}

