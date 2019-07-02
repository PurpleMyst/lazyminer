#[derive(Debug)]
pub enum Error {
    InvalidBooleanValue(u8),
    IoError(std::io::Error),
    Custom(String),
    HumongousString,
    HumongousVarInt,
    InvalidString,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use Error::*;

        match self {
            InvalidBooleanValue(n) => write!(
                f,
                "invalid value for boolean: expected 0x00 or 0x01, found {:#x}",
                n
            ),

            IoError(err) => write!(f, "{}", err.to_string()),

            HumongousString => write!(
                f,
                "tried to serialize string with a size larger than {}",
                i32::max_value()
            ),

            HumongousVarInt => write!(f, "tried to deserialize a VarInt with too many bytes"),

            InvalidString => write!(f, "string contained non-utf8 chars"),

            Custom(s) => write!(f, "{}", s),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Error::IoError(value)
    }
}

impl serde::ser::Error for Error {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        Error::Custom(format!("{}", msg))
    }
}

impl serde::de::Error for Error {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        serde::ser::Error::custom(msg)
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
