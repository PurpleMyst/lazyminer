use std::{borrow::Cow, convert::TryInto};

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SerializeError(&'static str);

type Result<T, E = SerializeError> = std::result::Result<T, E>;
type WithRemaining<'a, T> = (T, &'a [u8]);

pub(crate) trait Serialize {
    fn serialize(&self) -> Result<Cow<[u8]>>;
}

pub(crate) trait Deserialize: Sized {
    fn deserialize(buf: &[u8]) -> Result<WithRemaining<Self>>;
}

impl Serialize for bool {
    fn serialize(&self) -> Result<Cow<[u8]>> {
        Ok(Cow::from(if *self {
            &[0x01u8] as &[u8]
        } else {
            &[0x00u8] as &[u8]
        }))
    }
}

impl Deserialize for bool {
    fn deserialize(buf: &[u8]) -> Result<WithRemaining<Self>> {
        Ok((
            match buf[0] {
                0 => false,
                1 => true,
                _ => Err(SerializeError("invalid value for boolean"))?,
            },
            &buf[1..],
        ))
    }
}

macro_rules! coder_int_impl {
    ($($ty:ty),*) => {
        $(
        impl Serialize for $ty {
            fn serialize(&self) -> Result<Cow<[u8]>> {
                Ok(Cow::from((&self.to_be_bytes() as &[u8]).to_owned()))
            }
        }

        impl Deserialize for $ty {
            fn deserialize(buf: &[u8]) -> Result<WithRemaining<Self>> {
                Ok((
                    <$ty>::from_be_bytes(buf[0..std::mem::size_of::<$ty>()].try_into().unwrap()),
                    &buf[std::mem::size_of::<$ty>()..],
                ))
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
            fn serialize(&self) -> Result<Cow<[u8]>> {
                // I have no idea why I can't just replace this with `.to_owned()`.
                Ok(Cow::from(self.to_bits().serialize()?.into_owned()))
            }
        }

        impl Deserialize for $ty {
            fn deserialize(buf: &[u8]) -> Result<WithRemaining<Self>> {
                Deserialize::deserialize(buf).map(|(v, rest)| (<$ty>::from_bits(v), rest))
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
                fn serialize(&self) -> Result<Cow<[u8]>> {
                    if self.0 == 0 {
                        return Ok(Cow::from(&[0u8] as &[_]))
                    }

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

                    Ok(Cow::from(buf))
                }
            }

            impl Deserialize for $ty {
                fn deserialize(mut buf: &[u8]) -> Result<WithRemaining<Self>> {
                    let mut result = <$ty>::default();

                    for i in 0.. {
                        let done = match buf.get(0).cloned() {
                            Some(byte) => {
                                result.0 |= (byte & 0b01_11_11_11) as $inner_ty << (7 * i);
                                byte & 0b10_00_00_00 == 0
                            },

                            None => Err(SerializeError(concat!("Expected another byte for ", stringify!($ty),  " but didn't find one")))?,
                        };

                        buf = &buf[1..];

                        if done {
                            break;
                        }
                    }

                    Ok((result, buf))
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
            $(
            proptest!(|(x: $ty)| {
                let serialized = x.serialize().unwrap();
                prop_assert_eq!(
                    (x, &[] as &[u8]),
                    Deserialize::deserialize(&serialized).unwrap()
                );
            });
            )*
        };

        ($($var:ident: $ty:ty => $expr:expr),*) => {
            $(
            proptest!(|($var: $ty)| {
                let x = $expr;
                let serialized = x.serialize().unwrap();
                prop_assert_eq!(
                    (x, &[] as &[u8]),
                    Deserialize::deserialize(&serialized).unwrap()
                );
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
