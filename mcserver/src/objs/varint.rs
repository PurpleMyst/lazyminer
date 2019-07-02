use serde::{
    de::{self, Deserialize, Deserializer, Visitor},
    ser::{Serialize, Serializer},
};

macro_rules! coder_varint_impl {
    ($($ty:ident: $inner_ty:ty),*) => {
        $(
            #[derive(Default, Eq, PartialEq, Debug, Clone, Copy)]
            pub struct $ty(pub $inner_ty);

            impl Serialize for $ty {
                fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                    const MAX_BYTE_SIZE: usize = 1 + std::mem::size_of::<$ty>() * 8 / 7;

                    if self.0 == 0 {
                        return serializer.serialize_bytes(&[0u8] as &[_]);
                    }

                    let mut buf = smallvec::SmallVec::<[u8; MAX_BYTE_SIZE]>::new();

                    for i in 0..MAX_BYTE_SIZE {
                        let rest = self.0 >> (7 * i);
                        if rest == 0 {
                            break;
                        }

                        let lower = (rest & lsb!(7)) as u8;
                        buf.push(lower | 0b10_00_00_00);
                    }

                    *buf.last_mut().unwrap() ^= 0b10_00_00_00;

                    serializer.serialize_bytes(&buf)
                }
            }

            impl<'de> Deserialize<'de> for $ty {
                fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                    struct VarIntVisitor;

                    impl<'de> Visitor<'de> for VarIntVisitor {
                        type Value = $ty;

                        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                            write!(f, "A sequence of VarInt-encoded bytes")
                        }

                        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                            let mut result = <$ty>::default();

                            for i in 0.. {
                                let byte: u8 = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(i + 1, &self))?;

                                result.0 |= ((byte & 0b01_11_11_11) as $inner_ty).checked_shl(7 * i as u32).ok_or_else(|| de::Error::invalid_length(i + 1, &self))?;

                                if byte & 0b10_00_00_00 == 0 {
                                    break;
                                }
                            }

                            return Ok(result);
                        }
                    }

                    deserializer.deserialize_seq(VarIntVisitor)
                }
            }
        )*
    }
}

coder_varint_impl!(VarInt: i32, VarLong: i64);
