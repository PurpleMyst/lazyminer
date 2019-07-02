#![deny(unused_must_use)]

pub mod de;
pub mod error;
pub mod ser;

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    macro_rules! coder_roundtrip {
        ($expr:block$(: $expr_ty:ty)?) => {{
            let value = $expr;

            let mut cursor = std::io::Cursor::new(Vec::new());

            let mut serializer = crate::coder::ser::Serializer::new(&mut cursor);
            serde::ser::Serialize::serialize(&value, &mut serializer).unwrap();

            cursor.set_position(0);

            let deserialized$(: $expr_ty)? = {
                let mut deserializer = crate::coder::de::Deserializer::new(&mut cursor);
                serde::de::Deserialize::deserialize(&mut deserializer).unwrap()
            };

            prop_assert_eq!(value, deserialized);
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
        use crate::objs::{VarInt, VarLong};
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
            use crate::objs::Position;
            coder_roundtrip!({ Position { x, y: y as _, z } });
        }
    }
}
