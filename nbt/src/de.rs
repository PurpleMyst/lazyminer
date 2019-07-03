use std::{borrow::Cow, collections::VecDeque, convert::TryFrom, io::Read};

use serde::de::{self, Visitor};
use serde::forward_to_deserialize_any;

use crate::error::{Error, Result};

#[derive(Eq, Clone, Copy, PartialEq, Debug)]
enum DeserializerState {
    /// The deserializer is parsing a TAG_List and knows how many elements it has left and of what
    /// type they are.
    ListBeforeItem { size: usize, type_id: u8 },

    /// The deserializer is parsing a TAG_Compound and is positioned before the next entry.
    CompoundBeforeEntry,

    /// The deserializer is parsing a TAG_Compound and is positioned after the current entry's
    /// TypeID but before its name.
    CompoundBeforeEntryName { type_id: u8 },

    /// The deserializer is parsing a TAG_Compound and is positioned after the current entry's
    /// TypeID and Name.
    CompoundBeforeEntryPayload { type_id: u8 },
}

pub struct Deserializer<R: Read> {
    r: R,
    state: VecDeque<DeserializerState>,
}

impl<R: Read> Deserializer<R> {
    pub fn new(r: R) -> Self {
        Self {
            r,
            state: VecDeque::default(),
        }
    }
}
macro_rules! de_int {
    ($($meth:ident: $ty:ty),*) => {
        $(
        fn $meth(&mut self) -> Result<$ty> {
            let mut buf = [0; std::mem::size_of::<$ty>()];
            self.r.read_exact(&mut buf)?;
            Ok(<$ty>::from_be_bytes(buf))
        }
        )*
    };
}

macro_rules! de_float {
    ($($meth:ident: $parse_bits:ident => $ty:ty),*) => {
        $(
        fn $meth(&mut self) -> Result<$ty> {
            self.$parse_bits().map(<$ty>::from_bits)
        }
        )*
    };
}

impl<R: Read> Deserializer<R> {
    de_int!(
        parse_i8: i8,
        parse_i16: i16,
        parse_i32: i32,
        parse_i64: i64,
        parse_u32: u32,
        parse_u64: u64
    );

    de_float!(
        parse_f32: parse_u32 => f32,
        parse_f64: parse_u64 => f64
    );

    fn parse_usize<T>(&mut self, value: T, expected: &dyn de::Expected) -> Result<usize>
    where
        usize: TryFrom<T>,
        i64: From<T>,
        T: Copy,
    {
        usize::try_from(value).map_err(|_| {
            de::Error::invalid_type(de::Unexpected::Signed(i64::from(value)), expected)
        })
    }

    // FIXME: Avoid allocation here.
    fn parse_string(&mut self) -> Result<String> {
        let size = {
            let size_i16 = self.parse_i16()?;
            self.parse_usize(size_i16, &"the size of a string")?
        };
        let mut buf = vec![0; size];
        self.r.read_exact(&mut buf)?;

        cesu8::from_java_cesu8(&buf)
            .map(Cow::into_owned)
            .map_err(|_| de::Error::invalid_value(de::Unexpected::Bytes(&buf), &"a string"))
    }

    fn parse_type_id(&mut self) -> Result<u8> {
        let mut type_id_buf = [0u8; 1];
        self.r.read_exact(&mut type_id_buf)?;
        Ok(type_id_buf[0])
    }

    fn parse_tag_payload<'de, V: Visitor<'de>>(
        &mut self,
        visitor: V,
        type_id: u8,
    ) -> Result<V::Value> {
        match type_id {
            // TAG_Byte
            1 => visitor.visit_i8(self.parse_i8()?),

            // TAG_Short
            2 => visitor.visit_i16(self.parse_i16()?),

            // TAG_Int
            3 => visitor.visit_i32(self.parse_i32()?),

            // TAG_Long
            4 => visitor.visit_i64(self.parse_i64()?),

            // TAG_Float
            5 => visitor.visit_f32(self.parse_f32()?),

            // TAG_Double
            6 => visitor.visit_f64(self.parse_f64()?),

            // TAG_Byte_Array
            // This is technically an array of signed bytes, but just using `visit_bytes`
            // should be fine.
            7 => {
                let size = {
                    let size_i32 = self.parse_i32()?;
                    self.parse_usize(size_i32, &visitor)?
                };

                let mut buf = vec![0; size];
                self.r.read_exact(&mut buf)?;
                visitor.visit_bytes(&buf)
            }

            // TAG_String
            8 => visitor.visit_string(self.parse_string()?),

            // TAG_List
            9 => {
                let type_id = self.parse_type_id()?;
                let size = {
                    let size_i32 = self.parse_i32()?;
                    self.parse_usize(size_i32, &visitor)?
                };

                self.state
                    .push_back(DeserializerState::ListBeforeItem { size, type_id });

                visitor.visit_seq(&mut *self)
            }

            // TAG_Compound
            10 => {
                self.state.push_back(DeserializerState::CompoundBeforeEntry);
                visitor.visit_map(&mut *self)
            }

            _ => Err(de::Error::invalid_type(
                de::Unexpected::Unsigned(u64::from(type_id)),
                &visitor,
            )),
        }
    }
}

impl<'de, R: Read> de::Deserializer<'de> for &'_ mut Deserializer<R> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let type_id = match self.state.pop_back() {
            None => self.parse_type_id()?,

            Some(DeserializerState::CompoundBeforeEntry) => unreachable!(),

            Some(DeserializerState::CompoundBeforeEntryName { type_id }) => {
                self.state
                    .push_back(DeserializerState::CompoundBeforeEntryPayload { type_id });
                return visitor.visit_string(self.parse_string()?);
            }

            Some(state @ DeserializerState::ListBeforeItem { .. }) => {
                self.state.push_back(state);

                if let DeserializerState::ListBeforeItem { type_id, .. } = state {
                    type_id
                } else {
                    unreachable!()
                }
            }

            Some(DeserializerState::CompoundBeforeEntryPayload { type_id }) => {
                self.state.push_back(DeserializerState::CompoundBeforeEntry);
                type_id
            }
        };

        if self.state.is_empty() {
            // throw away name
            self.parse_string()?;
        }

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

impl<'de, R: Read> de::SeqAccess<'de> for Deserializer<R> {
    type Error = Error;

    fn next_element_seed<T: de::DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Self::Error> {
        if let Some(DeserializerState::ListBeforeItem { size, type_id }) = self.state.pop_back() {
            if size == 0 {
                return Ok(None);
            }

            self.state.push_back(DeserializerState::ListBeforeItem {
                size: size - 1,
                type_id,
            });

            seed.deserialize(self).map(Some)
        } else {
            Err(de::Error::custom("Invalid state in SeqAccess"))
        }
    }
}

impl<'de, R: Read> de::MapAccess<'de> for Deserializer<R> {
    type Error = Error;

    fn next_key_seed<K: de::DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Self::Error> {
        if let Some(DeserializerState::CompoundBeforeEntry) = self.state.pop_back() {
            let type_id = self.parse_type_id()?;

            // TAG_End
            if type_id == 0 {
                return Ok(None);
            }

            self.state
                .push_back(DeserializerState::CompoundBeforeEntryName { type_id });

            seed.deserialize(self).map(Some)
        } else {
            Err(de::Error::custom("Invalid state in next_key_seed"))
        }
    }

    fn next_value_seed<V: de::DeserializeSeed<'de>>(
        &mut self,
        seed: V,
    ) -> Result<V::Value, Self::Error> {
        if let Some(DeserializerState::CompoundBeforeEntryPayload { .. }) = self.state.back() {
            seed.deserialize(self)
        } else {
            Err(de::Error::custom("Invalid state in next_value_seed"))
        }
    }
}
