use std::{collections::VecDeque, convert::TryFrom, io::Write};

use serde::ser;

use crate::error::{Error, Result};

#[derive(Debug, Eq, PartialEq, Clone)]
enum State {
    /// The serializer is in a TAG_List with a known size and is positioned before the first item,
    /// so it doesn't know the TypeID of the list.
    FirstListItem { size: i32 },

    /// The serializer is in a TAG_List with a known size and TypeID and is positioned before the
    /// next item.
    InList { type_id: u8, size: i32 },

    /// The serializer is in a TAG_Compound and is positioned before the next named tag.
    CompoundBeforeEntry,

    /// The serializer is in a TAG_Compound and is positioned before the current named tag's
    /// payload but after its TypeID and name.
    CompoundBeforeEntryValue { name: String },
}

pub struct Serializer<W: Write> {
    w: W,
    state: VecDeque<State>,
}

impl<W: Write> Serializer<W> {
    pub fn new(w: W) -> Self {
        Self {
            w,
            state: VecDeque::new(),
        }
    }
}

macro_rules! ser_int_payload {
    ($($meth:ident: $ty:ty,)*) => {
        $(
        fn $meth(&mut self, v: $ty) -> Result<()> {
            self.w.write_all(&v.to_be_bytes()).map_err(Into::into)
        }
        )*
    }
}

macro_rules! ser_float_payload {
    ($($meth:ident: $ty:ty => $bits_payload:ident,)*) => {
        $(
        fn $meth(&mut self, v: $ty) -> Result<()> {
            self.$bits_payload(v.to_bits())
        }
        )*
    }
}

impl<W: Write> Serializer<W> {
    ser_int_payload!(
        serialize_i8_payload: i8,
        serialize_i16_payload: i16,
        serialize_i32_payload: i32,
        serialize_i64_payload: i64,
        serialize_u16_payload: u16,
        serialize_u32_payload: u32,
        serialize_u64_payload: u64,
    );

    ser_float_payload!(
        serialize_f32_payload: f32 => serialize_u32_payload,
        serialize_f64_payload: f64 => serialize_u64_payload,
    );

    fn serialize_bytearray_payload(&mut self, buf: &[u8]) -> Result<()> {
        self.serialize_i32_payload(
            i32::try_from(buf.len())
                .map_err(|_| Error::Message(String::from("Byte array too long for NBT format")))?,
        )?;

        self.w.write_all(&buf)?;

        Ok(())
    }

    fn serialize_string_payload(&mut self, s: &str) -> Result<()> {
        let buf = cesu8::to_java_cesu8(s);

        self.serialize_u16_payload(
            u16::try_from(buf.len())
                .map_err(|_| Error::Message(String::from("String too long for NBT format")))?,
        )?;

        self.w.write_all(&buf)?;

        Ok(())
    }

    fn serialize_type_id(&mut self, type_id: u8) -> Result<()> {
        match self.state.back_mut() {
            None
            | Some(State::CompoundBeforeEntryValue { .. })
            | Some(State::CompoundBeforeEntry) => self.w.write_all(&[type_id])?,

            Some(State::FirstListItem { size }) => {
                let size = size.clone();
                self.state.pop_back();
                self.state.push_back(State::InList {
                    // We subtract one from the size because we are in the process of serializing
                    // the first item already.
                    size: size - 1,
                    type_id,
                });

                self.w.write_all(&[type_id])?;
                self.serialize_i32_payload(size)?;
            }

            Some(State::InList {
                size,
                type_id: list_type_id,
            }) => {
                if type_id != *list_type_id {
                    return Err(Error::Message(String::from(
                        "Heterogenuous sequence not allowed in NBT format",
                    )));
                }

                // The serializer should stop us before we get here.
                if *size == 0 {
                    unreachable!();
                }

                *size -= 1;
            }
        }

        Ok(())
    }

    fn serialize_name(&mut self) -> Result<()> {
        // We `pop_back` from the state even though we just pop it back in the TAG_List case to
        // avoid a copy in the TAG_Compound case.
        match self.state.pop_back() {
            // If we're not in a TAG_List we must assign a name to every tag, but we don't have any
            // name to assign to tags not inside a TAG_Compound, so we just assign a dummy empty
            // name.
            None => self.serialize_string_payload(""),

            Some(state @ State::InList { .. }) | Some(state @ State::FirstListItem { .. }) => {
                self.state.push_back(state);
                Ok(())
            }

            Some(State::CompoundBeforeEntry) => unreachable!(),
            Some(State::CompoundBeforeEntryValue { name }) => {
                self.serialize_string_payload(&name)?;
                self.state.push_back(State::CompoundBeforeEntry);

                Ok(())
            }
        }
    }
}

macro_rules! ser_tag {
    ($(($type_id:literal, $serialize_payload:ident) => $meth:ident: $ty:ty,)*) => {
        $(
        fn $meth(self, v: $ty) -> Result<()> {
            self.serialize_type_id($type_id)?;
            self.serialize_name()?;
            self.$serialize_payload(v)?;

            Ok(())
        }
        )*
    };
}

macro_rules! ser_unsupported {
    ($($meth:ident$(: $ty:ty)?,)*) => {
        $(
        fn $meth(self$(, _v: $ty)?) -> Result<()> {
            Err(ser::Error::custom(concat!("Unsupported method for NBT format: ", stringify!($meth))))
        }
        )*
    }
}

impl<W: Write> ser::Serializer for &'_ mut Serializer<W> {
    type Ok = ();
    type Error = Error;

    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = ser::Impossible<Self::Ok, Self::Error>;
    type SerializeTupleVariant = ser::Impossible<Self::Ok, Self::Error>;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = ser::Impossible<Self::Ok, Self::Error>;

    ser_tag!(
        (1, serialize_i8_payload) => serialize_i8: i8,
        (2, serialize_i16_payload) => serialize_i16: i16,
        (3, serialize_i32_payload) => serialize_i32: i32,
        (4, serialize_i64_payload) => serialize_i64: i64,
        (5, serialize_f32_payload) => serialize_f32: f32,
        (6, serialize_f64_payload) => serialize_f64: f64,
        (7, serialize_bytearray_payload) => serialize_bytes: &[u8],
    );

    fn serialize_str(self, v: &str) -> Result<()> {
        if let Some(State::CompoundBeforeEntry) = self.state.back() {
            self.state.pop_back();
            self.state
                .push_back(State::CompoundBeforeEntryValue { name: v.to_owned() });
            return Ok(());
        }

        self.serialize_type_id(8)?;
        self.serialize_name()?;
        self.serialize_string_payload(v)?;

        Ok(())
    }

    fn serialize_none(self) -> Result<()> {
        Ok(())
    }

    fn serialize_some<T: ?Sized + ser::Serialize>(self, value: &T) -> Result<()> {
        value.serialize(self)
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _enum_name: &'static str,
        index: u32,
        _variant: &'static str,
    ) -> Result<()> {
        self.serialize_u32(index)
    }

    fn serialize_newtype_struct<T: ?Sized + ser::Serialize>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<()> {
        value.serialize(self)
    }

    // Note that newtype variant (and all of the other variant serialization
    // methods) refer exclusively to the "externally tagged" enum
    // representation.
    fn serialize_newtype_variant<T: ?Sized + ser::Serialize>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<()> {
        unimplemented!()
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq> {
        if let Some(len) = len {
            self.serialize_tuple(len)
        } else {
            Err(Error::Message(String::from("size must be known")))
        }
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple> {
        self.serialize_type_id(9)?;
        self.serialize_name()?;
        self.state.push_back(State::FirstListItem {
            size: i32::try_from(len).map_err(|_| Error::Message(String::from("tuple too long")))?,
        });
        Ok(self)
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        unimplemented!()
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        unimplemented!()
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        self.serialize_type_id(10)?;
        self.serialize_name()?; // XXX: remove this to text context
        self.state.push_back(State::CompoundBeforeEntry);
        Ok(self)
    }

    fn serialize_struct(self, _name: &'static str, len: usize) -> Result<Self::SerializeStruct> {
        self.serialize_map(Some(len))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        unimplemented!();
    }

    ser_unsupported!(
        serialize_u8: u8,
        serialize_u16: u16,
        serialize_u32: u32,
        serialize_u64: u64,
        serialize_char: char,
        serialize_bool: bool,
        serialize_unit,
    );
}

impl<W: Write> ser::SerializeSeq for &'_ mut Serializer<W> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: ?Sized + ser::Serialize>(&mut self, value: &T) -> Result<()> {
        ser::SerializeTuple::serialize_element(self, value)
    }

    fn end(self) -> Result<()> {
        ser::SerializeTuple::end(self)
    }
}

impl<W: Write> ser::SerializeTuple for &'_ mut Serializer<W> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: ?Sized + ser::Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        match self.state.back() {
            Some(State::InList { size, .. }) if *size == 0 => {
                self.state.pop_back();
            }

            Some(State::FirstListItem { size }) if *size == 0 => {
                // If we get to the end without switching to the `InList` state, it means we have
                // not serialized a single element, because this list is empty, and so we haven't serialized any TypeID nor
                // size. Since by the spec we must serialize a TypeID even for the empty list, we
                // utilize the TAG_End TypeID because it feels right.
                let size = *size;
                self.w.write_all(&[0])?;
                self.serialize_i32_payload(size)?;

                self.state.pop_back();
            }

            _ => unreachable!(),
        }

        Ok(())
    }
}

impl<W: Write> ser::SerializeMap for &'_ mut Serializer<W> {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T: ?Sized + ser::Serialize>(&mut self, key: &T) -> Result<()> {
        key.serialize(&mut **self)
    }

    fn serialize_value<T: ?Sized + ser::Serialize>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        self.serialize_type_id(0)?;
        assert_eq!(self.state.pop_back(), Some(State::CompoundBeforeEntry));
        Ok(())
    }
}

impl<W: Write> ser::SerializeStruct for &'_ mut Serializer<W> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + ser::Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        ser::SerializeMap::serialize_key(self, key)?;
        ser::SerializeMap::serialize_value(self, value)?;

        Ok(())
    }

    fn end(self) -> Result<()> {
        ser::SerializeMap::end(self)
    }
}
