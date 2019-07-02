use std::{io::Read, mem::size_of};

use serde::de::{self, Deserialize, Visitor};

use super::{
    super::objs::VarInt,
    error::{Error, Result},
};

pub struct Deserializer<R: Read>(R);

impl<R: Read> Deserializer<R> {
    pub fn new(r: R) -> Self {
        Self(r)
    }
}

macro_rules! de_int {
    ($($name:ident: $ty:ty => $visitor_method:ident),*) => {
        $(
        fn $name<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
            let mut buf = [0; size_of::<$ty>()];
            self.0.read_exact(&mut buf)?;

            visitor.$visitor_method(<$ty>::from_be_bytes(buf))
        }
        )*
    };
}

macro_rules! de_float {
    ($($name:ident: $bits_ty:ty => $ty:ty => $visitor_method:ident),*) => {
        $(
        fn $name<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
            let mut buf = [0; size_of::<$bits_ty>()];
            self.0.read_exact(&mut buf)?;

            visitor.$visitor_method(<$ty>::from_bits(<$bits_ty>::from_be_bytes(buf)))
        }
        )*
    };
}

impl<'de, R: Read> de::Deserializer<'de> for &'_ mut Deserializer<R> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unimplemented!()
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let mut buf = [0; 1];
        self.0.read_exact(&mut buf)?;

        let value = match buf[0] {
            0 => false,
            1 => true,
            _ => {
                return Err(de::Error::invalid_value(
                    de::Unexpected::Bytes(&buf),
                    &visitor,
                ))
            }
        };

        visitor.visit_bool(value)
    }

    de_int!(
        deserialize_i8: i8 => visit_i8,
        deserialize_i16: i16 => visit_i16,
        deserialize_i32: i32 => visit_i32,
        deserialize_i64: i64 => visit_i64,
        deserialize_i128: i128 => visit_i128
    );

    de_int!(
        deserialize_u8: u8 => visit_u8,
        deserialize_u16: u16 => visit_u16,
        deserialize_u32: u32 => visit_u32,
        deserialize_u64: u64 => visit_u64,
        deserialize_u128: u128 => visit_u128
    );

    de_float!(
        deserialize_f32: u32 => f32 => visit_f32,
        deserialize_f64: u64 => f64 => visit_f64
    );

    fn deserialize_char<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unimplemented!()
    }

    fn deserialize_str<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unimplemented!()
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let size = VarInt::deserialize(&mut *self)?.0;

        visitor.visit_string(
            (0..size)
                .map(|_| {
                    u32::deserialize(&mut *self)
                        .and_then(|c| std::char::from_u32(c).ok_or(Error::InvalidString))
                })
                .collect::<Result<String>>()?,
        )
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unimplemented!()
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unimplemented!()
    }

    fn deserialize_option<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unimplemented!()
    }

    fn deserialize_unit<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unimplemented!()
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _visitor: V,
    ) -> Result<V::Value> {
        unimplemented!()
    }

    // As is done here, serializers are encouraged to treat newtype structs as
    // insignificant wrappers around the data they contain. That means not
    // parsing anything other than the contained value.
    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _visitor: V,
    ) -> Result<V::Value> {
        unimplemented!()
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        struct SeqAccess<'a, R: Read>(&'a mut Deserializer<R>);

        impl<'de, R: Read> de::SeqAccess<'de> for SeqAccess<'_, R> {
            type Error = Error;

            fn next_element_seed<T: de::DeserializeSeed<'de>>(
                &mut self,
                seed: T,
            ) -> Result<Option<T::Value>> {
                seed.deserialize(&mut *self.0).map(Some)
            }
        }

        visitor.visit_seq(SeqAccess(&mut *self))
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, _len: usize, _visitor: V) -> Result<V::Value> {
        unimplemented!()
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _len: usize,
        _visitor: V,
    ) -> Result<V::Value> {
        unimplemented!()
    }

    fn deserialize_map<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unimplemented!()
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value> {
        unimplemented!()
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value> {
        unimplemented!()
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unimplemented!()
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        unimplemented!()
    }
}
