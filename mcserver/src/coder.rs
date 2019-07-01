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

macro_rules! lsb {
    ($n:expr) => {
        (1 << $n) - 1
    };
}

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

coder_int_impl!(i8, u8, i16, u16, i32, i64, u128);

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

                        let lower = (rest & lsb!(7)) as u8;
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

#[derive(Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Debug)]
pub struct Position {
    x: i32, // 26 bits
    y: i16, // 12 bits
    z: i32, // 26 bits
}

impl Serialize for Position {
    fn serialize(&self, w: impl Write) -> Result<()> {
        let x: u64 = self.x as u64 & lsb!(26);
        let y: u64 = self.y as u64 & lsb!(12);
        let z: u64 = self.z as u64 & lsb!(26);

        let n: u64 = (x << (26 + 12)) | (y << 26) | z;

        n.serialize(w)
    }
}

impl Deserialize for Position {
    fn deserialize(r: impl Read) -> Result<Self> {
        let n = u64::deserialize(r)?;

        let z = (n & lsb!(26)) as u32;
        let y = ((n >> 26) & lsb!(12)) as u16;
        let x = ((n >> (26 + 12)) & lsb!(26)) as u32;

        macro_rules! uN_to_iN {
            ($x:ident: $N:literal; $from_ty:ty => $to_ty:ty) => {
                $x as $to_ty
                    - if $x >= (2 as $from_ty).pow($N - 1) {
                        (2 as $to_ty).pow($N)
                    } else {
                        0
                    }
            };
        };

        let x = uN_to_iN!(x: 26; u32 => i32);
        let y = uN_to_iN!(y: 12; u16 => i16);
        let z = uN_to_iN!(z: 26; u32 => i32);

        Ok(Self { x, y, z })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    macro_rules! coder_roundtrip {
        ($expr:block$(: $expr_ty:ty)?) => {{
            use std::io::{Cursor, Seek, SeekFrom};
            let x = $expr;

            let mut cursor = Cursor::new(Vec::new());
            x.serialize(&mut cursor).unwrap();
            cursor.seek(SeekFrom::Start(0)).unwrap();

            let deserialized$(: $expr_ty)? = Deserialize::deserialize(&mut cursor).unwrap();
            prop_assert_eq!(x, deserialized);
        }};
    }

    macro_rules! coder_roundtrip_proptest {
        ($($ty:ty),*) => {
            coder_roundtrip_proptest!($(x: $ty => { x }: $ty),*);
        };

        ($($var:ident: $ty:ty => $expr:block$(: $expr_ty:ty)?),*) => {
            $( proptest!(|($var: $ty)| coder_roundtrip!($expr$(: $expr_ty)?)); )*
        };
    }

    #[test]
    fn test_bool() {
        coder_roundtrip_proptest!(bool);
    }

    #[test]
    fn test_ints() {
        coder_roundtrip_proptest!(i8, u8, i16, u16, i32, i64, u32, u64, u128);
    }

    #[test]
    fn test_floats() {
        coder_roundtrip_proptest!(f32, f64);
    }

    #[test]
    fn test_varint() {
        coder_roundtrip_proptest!(n: i32 => { VarInt(n) }, n: i64 => { VarLong(n) });
    }

    #[test]
    fn test_string() {
        coder_roundtrip_proptest!(String);
    }

    macro_rules! signed_int_range {
        ($bits:literal) => {
            -(1 << ($bits - 1))..(1 << ($bits - 1)) - 1
        };
    }

    proptest! {
        #[test]
        fn test_position(x in signed_int_range!(26), y in signed_int_range!(12), z in signed_int_range!(26)) {
            coder_roundtrip!({ Position { x, y: y as _, z } });
        }
    }
}
