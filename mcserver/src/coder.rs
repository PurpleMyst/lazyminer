#![deny(unused_must_use)]

use std::{
    io::{Read, Write},
    mem::size_of,
};

#[derive(Debug)]
pub(crate) enum Error {
    InvalidBooleanValue(u8),
    NotEnoughBytesForVarInt,
    IoError(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::InvalidBooleanValue(n) => write!(
                f,
                "Invalid value for boolean: expected 0x00 or 0x01, found {:#x}",
                n
            ),

            Error::NotEnoughBytesForVarInt => write!(f, "Not enough bytes for VarInt"),

            Error::IoError(err) => write!(f, "{}", err.to_string()),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Error::IoError(value)
    }
}

type Result<T, E = Error> = std::result::Result<T, E>;

pub(crate) trait Serialize {
    fn serialize(&self, w: impl Write) -> Result<()>;
}

pub(crate) trait Deserialize: Sized {
    fn deserialize(r: impl Read) -> Result<Self>;
}

impl Serialize for bool {
    fn serialize(&self, mut w: impl Write) -> Result<()> {
        w.write_all(&[if *self { 0x01u8 } else { 0x00u8 }] as &[_])?;

        Ok(())
    }
}

impl Deserialize for bool {
    fn deserialize(mut r: impl Read) -> Result<Self> {
        let mut buf = [0; 1];
        r.read_exact(&mut buf)?;

        Ok(match buf[0] {
            0 => false,
            1 => true,
            n => Err(Error::InvalidBooleanValue(n))?,
        })
    }
}

macro_rules! coder_int_impl {
    ($($ty:ty),*) => {
        $(
        impl Serialize for $ty {
            fn serialize(&self, mut w: impl Write) -> Result<()> {
                w.write_all(&self.to_be_bytes())?;
                Ok(())
            }
        }

        impl Deserialize for $ty {
            fn deserialize(mut r: impl Read) -> Result<Self> {
                let mut buf = [0; size_of::<$ty>()];
                r.read_exact(&mut buf)?;

                Ok(<$ty>::from_be_bytes(buf))
            }
        }
        )*
    };
}

coder_int_impl!(i8, u8, i16, u16, i32, i64);

macro_rules! coder_float_impl {
    ($($ty:ty => $bit_ty:ty),*) => {
        $(

        // we need the types for floats but they aren't needed in the spec.
        coder_int_impl!($bit_ty);

        impl Serialize for $ty {
            fn serialize(&self, w: impl Write) -> Result<()> {
                self.to_bits().serialize(w)
            }
        }

        impl Deserialize for $ty {
            fn deserialize(r: impl Read) -> Result<Self> {
                let bits = Deserialize::deserialize(r)?;
                Ok(<$ty>::from_bits(bits))
            }
        }
        )*
    };
}

coder_float_impl!(f32 => u32, f64 => u64);

macro_rules! coder_varint_impl {
    ($($ty:ident: $inner_ty:ty),*) => {
        $(
            #[derive(Default, Eq, PartialEq, Debug, Clone, Copy)]
            pub struct $ty(pub $inner_ty);

            impl Serialize for $ty {
                fn serialize(&self, mut w: impl Write) -> Result<()> {
                    if self.0 == 0 {
                        w.write_all(&[0])?;

                        return Ok(());
                    }

                    // TODO: Use a SmallVec or just a buffer since the size is known
                    let mut buf = Vec::new();

                    for i in 0..=std::mem::size_of::<$ty>() * 8 / 7 {
                        let rest = self.0 >> (7 * i);
                        if rest == 0 {
                            break;
                        }

                        let lower = (rest & 0b01_11_11_11) as u8;
                        buf.push(lower | 0b10_00_00_00);
                    }

                    *buf.last_mut().unwrap() ^= 0b10_00_00_00;

                    w.write_all(&buf)?;

                    Ok(())
                }
            }

            impl Deserialize for $ty {
                fn deserialize(mut r: impl Read) -> Result<Self> {
                    let mut result = <$ty>::default();

                    let mut buf = [0; 1];
                    for i in 0.. {
                        r.read_exact(&mut buf)?;

                        let done = match buf.get(0).cloned() {
                            Some(byte) => {
                                result.0 |= (byte & 0b01_11_11_11) as $inner_ty << (7 * i);
                                byte & 0b10_00_00_00 == 0
                            },

                            None => Err(Error::NotEnoughBytesForVarInt)?,
                        };

                        if done {
                            break;
                        }
                    }

                    Ok(result)
                }
            }
        )*
    }
}

coder_varint_impl!(VarInt: i32, VarLong: i64);

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    macro_rules! coder_roundtrip {
        ($($ty:ty),*) => {
            coder_roundtrip!($(x: $ty => x),*);
        };

        ($($var:ident: $ty:ty => $expr:expr),*) => {
            $(
            proptest!(|($var: $ty)| {
                use std::io::{Cursor, Seek, SeekFrom};
                let x = $expr;

                let mut cursor = Cursor::new(Vec::new());
                x.serialize(&mut cursor).unwrap();
                cursor.seek(SeekFrom::Start(0)).unwrap();

                prop_assert_eq!(x, Deserialize::deserialize(&mut cursor).unwrap());
            });
            )*
        };
    }

    #[test]
    fn test_bool() {
        coder_roundtrip!(bool);
    }

    #[test]
    fn test_ints() {
        coder_roundtrip!(i8, u8, i16, u16, i32, i64);
    }

    #[test]
    fn test_floats() {
        coder_roundtrip!(f32, f64);
    }

    #[test]
    fn test_varint() {
        coder_roundtrip!(n: i32 => VarInt(n), n: i64 => VarLong(n));
    }
}
