use std::{
    fmt::{self, Display},
    io,
};

use serde::{de, ser};

#[derive(Debug)]
pub enum Error {
    Message(String),
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Message(s) => write!(f, "{}", s),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Error::Message(format!("{}", error))
    }
}

impl ser::Error for Error {
    fn custom<M: Display>(msg: M) -> Self {
        Error::Message(format!("{}", msg))
    }
}

impl de::Error for Error {
    fn custom<M: Display>(msg: M) -> Self {
        ser::Error::custom(msg)
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
