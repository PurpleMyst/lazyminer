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
                <$bit_ty>::deserialize(buf).map(|(v, rest)| (<$ty>::from_bits(v), rest))
            }
        }
        )*
    };
}

coder_float_impl!(f32 => u32, f64 => u64);

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
                    <$ty>::deserialize(&serialized).unwrap()
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
}
