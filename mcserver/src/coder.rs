#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SerializeError(&'static str);

type Result<T, E = SerializeError> = std::result::Result<T, E>;
type WithRemaining<'a, T> = (T, &'a [u8]);

pub(crate) trait Serialize {
    fn serialize(&self) -> Result<&[u8]>;
}

pub(crate) trait Deserialize: Sized {
    fn deserialize(buf: &[u8]) -> Result<WithRemaining<Self>>;
}

impl Serialize for bool {
    fn serialize(&self) -> Result<&[u8]> {
        if *self {
            Ok(&[0x01])
        } else {
            Ok(&[0x00])
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_bool(b: bool) {
            assert_eq!((b, &[] as &[u8]), bool::deserialize(b.serialize().unwrap()).unwrap());
        }
    }
}
