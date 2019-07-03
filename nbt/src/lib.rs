pub mod de;
pub mod error;
pub mod ser;

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use serde_value::Value;

    macro_rules! roundtrip {
        ($expr:expr $(; $expr_ty:ty)?) => {{
            let value = $expr;

            let mut cursor = std::io::Cursor::new(Vec::new());

            let mut serializer = crate::ser::Serializer::new(&mut cursor);
            serde::ser::Serialize::serialize(&value, &mut serializer).unwrap();

            cursor.set_position(0);

            let deserialized$(: $expr_ty)? = {
                let mut deserializer = crate::de::Deserializer::new(&mut cursor);
                serde::de::Deserialize::deserialize(&mut deserializer).unwrap()
            };

            prop_assert_eq!(value, deserialized);
        }};
    }

    macro_rules! values {
        ($(Value::$variant:ident($ty:ty),)*) => {
            prop_oneof![
                $(
                any::<$ty>().prop_map(Value::$variant),
                )*
            ]
        }
    }

    macro_rules! vecs {
        ($(Value::$variant:ident($ty:ty),)*) => {
            prop_oneof![
                $(
                prop::collection::vec(any::<$ty>().prop_map(Value::$variant), 0..256).prop_map(Value::Seq),
                )*
            ]
        }
    }

    fn primitive_value_strategy() -> impl Strategy<Value = Value> {
        values![
            Value::I8(i8),
            Value::I16(i16),
            Value::I32(i32),
            Value::I64(i64),
            Value::F32(f32),
            Value::F64(f64),
            Value::String(String),
        ]
    }

    fn vec_of_value_strategy() -> impl Strategy<Value = Value> {
        vecs![
            Value::I8(i8),
            Value::I16(i16),
            Value::I32(i32),
            Value::I64(i64),
            Value::F32(f32),
            Value::F64(f64),
            Value::String(String),
        ]
    }

    proptest! {
        #[test]
        fn test_primitives(v in primitive_value_strategy()) {
            roundtrip!(v);
        }

        #[test]
        fn test_vecs(v in vec_of_value_strategy()) {
            roundtrip!(v);
        }
    }

    #[test]
    fn test_empty_vec() -> Result<(), TestCaseError> {
        let empty: Vec<()> = Vec::new();
        roundtrip!(empty; Vec<()>);
        Ok(())
    }
}
