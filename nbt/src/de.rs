use std::{convert::TryFrom, io::Read};

use serde::de::{self, Visitor};
use serde::forward_to_deserialize_any;

use crate::error::{Error, Result};

pub struct Deserializer<R: Read>(R);

impl<R: Read> Deserializer<R> {
    pub fn new(r: R) -> Self {
        Self(r)
    }
}

impl<R: Read> Deserializer<R> {
    fn parse_tag_payload<'de, V: Visitor<'de>>(
        &'de mut self,
        visitor: V,
        type_id: u8,
    ) -> Result<V::Value> {
        macro_rules! de_int {
            ($ty:ty) => {{
                let mut buf = [0; std::mem::size_of::<$ty>()];
                self.0.read_exact(&mut buf)?;
                <$ty>::from_be_bytes(buf)
            }};
        }

        macro_rules! de_float {
            ($bits_ty:ty => $ty:ty) => {
                <$ty>::from_bits(de_int!($bits_ty))
            };
        }

        macro_rules! de_size {
            ($ty:ty) => {{
                let size_signed = de_int!($ty);
                let size: Result<usize> = usize::try_from(size_signed).map_err(|_| {
                    de::Error::invalid_type(de::Unexpected::Signed(size_signed as i64), &visitor)
                });

                size?
            }};
        }

        match type_id {
            // TAG_Byte
            1 => visitor.visit_i8(de_int!(i8)),

            // TAG_Short
            2 => visitor.visit_i16(de_int!(i16)),

            // TAG_Int
            3 => visitor.visit_i32(de_int!(i32)),

            // TAG_Long
            4 => visitor.visit_i64(de_int!(i64)),

            // TAG_Float
            5 => visitor.visit_f32(de_float!(u32 => f32)),

            // TAG_Double
            6 => visitor.visit_f64(de_float!(u64 => f64)),

            // TAG_Byte_Array
            // This is technically an array of signed bytes, but just using `visit_bytes`
            // should be fine.
            7 => {
                let size = de_size!(i32);
                let mut buf = vec![0; size];
                self.0.read_exact(&mut buf)?;
                visitor.visit_bytes(&buf)
            }

            // TAG_String
            8 => {
                use std::borrow::Cow;

                let size = de_size!(i16);
                let mut buf = vec![0; size];
                self.0.read_exact(&mut buf)?;
                let s: Result<Cow<str>> = cesu8::from_java_cesu8(&buf)
                    .map_err(|_| de::Error::invalid_value(de::Unexpected::Bytes(&buf), &visitor));

                visitor.visit_str(&s?)
            }

            9 => {
                let type_id = {
                    let mut type_id_buf = [0u8; 1];
                    self.0.read_exact(&mut type_id_buf)?;
                    type_id_buf[0]
                };

                let size = de_size!(i32);

                visitor.visit_seq(ListTag {
                    de: &mut *self,
                    type_id,
                    size,
                })
            }

            _ => Err(de::Error::invalid_type(
                de::Unexpected::Unsigned(type_id as u64),
                &visitor,
            )),
        }
    }
}

impl<'de, 'a: 'de, R: Read> de::Deserializer<'de> for &'a mut Deserializer<R> {
    type Error = Error;

    // Look at the input data to decide what Serde data model type to
    // deserialize as. Not all data formats are able to support this operation.
    // Formats that support `deserialize_any` are known as self-describing.
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let type_id = {
            let mut type_id_buf = [0u8; 1];
            self.0.read_exact(&mut type_id_buf)?;
            type_id_buf[0]
        };

        match type_id {
            _ => self.parse_tag_payload(visitor, type_id),
        }
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

struct ListTag<'a, R: Read> {
    de: &'a mut Deserializer<R>,
    size: usize,
    type_id: u8,
}

impl<'de, R: Read> de::Deserializer<'de> for ListTag<'de, R> {
    type Error = Error;

    // Look at the input data to decide what Serde data model type to
    // deserialize as. Not all data formats are able to support this operation.
    // Formats that support `deserialize_any` are known as self-describing.
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.de.parse_tag_payload(visitor, self.type_id)
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

impl<'de, 'a: 'de, R: Read> de::SeqAccess<'de> for ListTag<'de, R> {
    type Error = Error;

    fn next_element_seed<T: de::DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Self::Error> {
        if self.size == 0 {
            return Ok(None);
        }

        self.size -= 1;
        seed.deserialize(&mut *self).map(Some)
    }
}
