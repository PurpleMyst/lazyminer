#![deny(unused_must_use)]

use std::{
    io::{Read, Write},
    mem::size_of,
};

use smallvec::SmallVec;

#[derive(Debug)]
pub(crate) enum Error {
    InvalidBooleanValue(u8),
    IoError(std::io::Error),
    HumongousString,
    HumongousVarInt,
    InvalidString,
}

type Result<T, E = Error> = std::result::Result<T, E>;

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use Error::*;

        match self {
            InvalidBooleanValue(n) => write!(
                f,
                "Invalid value for boolean: expected 0x00 or 0x01, found {:#x}",
                n
            ),

            IoError(err) => write!(f, "{}", err.to_string()),

            HumongousString => write!(
                f,
                "Tried to serialize string with a size larger than {}",
                i32::max_value()
            ),

            HumongousVarInt => write!(f, "Tried to deserialize a VarInt with too many bytes"),

            InvalidString => write!(f, "String contained non-utf8 chars."),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Error::IoError(value)
    }
}

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
                w.write_all(&self.to_be_bytes()).map_err(Into::into)
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
                Deserialize::deserialize(r).map(<$ty>::from_bits)
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
                    const MAX_BYTE_SIZE: usize = 1 + std::mem::size_of::<$ty>() * 8 / 7;

                    if self.0 == 0 {
                        return w.write_all(&[0]).map_err(Into::into);
                    }

                    let mut buf = SmallVec::<[u8; MAX_BYTE_SIZE]>::new();

                    for i in 0..MAX_BYTE_SIZE {
                        let rest = self.0 >> (7 * i);
                        if rest == 0 {
                            break;
                        }

                        let lower = (rest & 0b01_11_11_11) as u8;
                        buf.push(lower | 0b10_00_00_00);
                    }

                    *buf.last_mut().unwrap() ^= 0b10_00_00_00;

                    w.write_all(&buf).map_err(Into::into)
                }
            }

            impl Deserialize for $ty {
                fn deserialize(mut r: impl Read) -> Result<Self> {
                    let mut result = <$ty>::default();

                    let mut buf = [0; 1];
                    for i in 0.. {
                        r.read_exact(&mut buf)?;

                        result.0 |= ((buf[0] & 0b01_11_11_11) as $inner_ty).checked_shl(7 * i).ok_or(Error::HumongousVarInt)?;

                        if buf[0] & 0b10_00_00_00 == 0 {
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

impl Serialize for String {
    fn serialize(&self, mut w: impl Write) -> Result<()> {
        use std::convert::TryInto;

        let cs = self.chars().map(|c| c as u32).collect::<Vec<_>>();

        cs.len()
            .try_into()
            .map(VarInt)
            .map_err(|_| Error::HumongousString)?
            .serialize(&mut w)?;

        cs.into_iter()
            .map(|c| c.serialize(&mut w))
            .collect::<Result<()>>()
    }
}

impl Deserialize for String {
    fn deserialize(mut r: impl Read) -> Result<Self> {
        let size = VarInt::deserialize(&mut r)?.0;

        (0..size)
            .map(|_| {
                u32::deserialize(&mut r)
                    .and_then(|c| std::char::from_u32(c).ok_or(Error::InvalidString))
            })
            .collect::<Result<_>>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    macro_rules! coder_roundtrip {
        ($($ty:ty),*) => {
            coder_roundtrip!($(x: $ty => { x }: $ty),*);
        };

        ($($var:ident: $ty:ty => $expr:block$(: $expr_ty:ty)?),*) => {
            $(
            proptest!(|($var: $ty)| {
                use std::io::{Cursor, Seek, SeekFrom};
                let x = $expr;

                let mut cursor = Cursor::new(Vec::new());
                x.serialize(&mut cursor).unwrap();
                cursor.seek(SeekFrom::Start(0)).unwrap();

                let deserialized$(: $expr_ty)? = Deserialize::deserialize(&mut cursor).unwrap();
                assert_eq!(x, deserialized);
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
        coder_roundtrip!(n: i32 => { VarInt(n) }, n: i64 => { VarLong(n) });
    }

    #[test]
    fn test_string() {
        coder_roundtrip!(String);
    }
}
