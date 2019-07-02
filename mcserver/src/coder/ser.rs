use std::io::Write;

use serde::ser::{self, Serialize};

use super::{
    super::objs::VarInt,
    error::{Error, Result},
};

pub struct Serializer<W: Write>(W);

impl<W: Write> Serializer<W> {
    pub fn new(w: W) -> Self {
        Self(w)
    }
}

macro_rules! ser_int {
    ($($name:ident: $ty:ty),*) => {
        $(
        fn $name(self, v: $ty) -> Result<()> {
            self.0.write_all(&v.to_be_bytes()).map_err(Into::into)
        }
        )*
    }
}

macro_rules! ser_float {
    ($($name:ident: $ty:ty),*) => {
        $(
            fn $name(self, v: $ty) -> Result<()> {
                v.to_bits().serialize(&mut *self)
            }
        )*
    }
}

pub enum NoSerialize {}

impl serde::ser::SerializeMap for NoSerialize {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T: ?Sized>(&mut self, _key: &T) -> Result<()> {
        unreachable!()
    }

    fn serialize_value<T: ?Sized>(&mut self, _value: &T) -> Result<()> {
        unreachable!()
    }

    fn end(self) -> Result<()> {
        unreachable!()
    }
}

impl serde::ser::SerializeTuple for NoSerialize {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: ?Sized>(&mut self, _element: &T) -> Result<()> {
        unreachable!()
    }

    fn end(self) -> Result<()> {
        unreachable!()
    }
}

impl serde::ser::SerializeTupleStruct for NoSerialize {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized>(&mut self, _element: &T) -> Result<()> {
        unreachable!()
    }

    fn end(self) -> Result<()> {
        unreachable!()
    }
}

impl serde::ser::SerializeTupleVariant for NoSerialize {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized>(&mut self, _element: &T) -> Result<()> {
        unreachable!()
    }

    fn end(self) -> Result<()> {
        unreachable!()
    }
}

impl serde::ser::SerializeStruct for NoSerialize {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized>(&mut self, _key: &'static str, _value: &T) -> Result<()> {
        unreachable!()
    }

    fn end(self) -> Result<()> {
        unreachable!()
    }
}

impl serde::ser::SerializeStructVariant for NoSerialize {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized>(&mut self, _key: &'static str, _value: &T) -> Result<()> {
        unreachable!()
    }

    fn end(self) -> Result<()> {
        unreachable!()
    }
}

impl<W: Write> ser::Serializer for &'_ mut Serializer<W> {
    type Ok = ();

    type Error = Error;

    type SerializeSeq = Self;
    type SerializeTuple = NoSerialize;
    type SerializeTupleStruct = NoSerialize;
    type SerializeTupleVariant = NoSerialize;
    type SerializeMap = NoSerialize;
    type SerializeStruct = NoSerialize;
    type SerializeStructVariant = NoSerialize;

    fn serialize_bool(self, v: bool) -> Result<()> {
        self.0
            .write_all(&[if v { 0x01u8 } else { 0x00u8 }] as &[_])
            .map_err(Into::into)
    }

    ser_int!(
        serialize_i8: i8,
        serialize_i16: i16,
        serialize_i32: i32,
        serialize_i64: i64,
        serialize_i128: i128
    );

    ser_int!(
        serialize_u8: u8,
        serialize_u16: u16,
        serialize_u32: u32,
        serialize_u64: u64,
        serialize_u128: u128
    );

    ser_float!(serialize_f32: f32, serialize_f64: f64);

    fn serialize_char(self, _v: char) -> Result<()> {
        unimplemented!();
    }

    fn serialize_str(self, v: &str) -> Result<()> {
        use std::convert::TryFrom;

        let cs = v.chars().map(|c| c as u32).collect::<Vec<_>>();

        i32::try_from(cs.len())
            .map(VarInt)
            .map_err(|_| Error::HumongousString)?
            .serialize(&mut *self)?;

        cs.into_iter()
            .map(|c| c.serialize(&mut *self))
            .collect::<Result<()>>()
    }

    // Useful for VarInt and VarLong
    fn serialize_bytes(self, v: &[u8]) -> Result<()> {
        self.0.write_all(v).map_err(Into::into)
    }

    fn serialize_none(self) -> Result<()> {
        Ok(())
    }

    fn serialize_some<T>(self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<()> {
        Ok(())
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

    fn serialize_newtype_struct<T>(self, _name: &'static str, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(self)
    }

    // Note that newtype variant (and all of the other variant serialization
    // methods) refer exclusively to the "externally tagged" enum
    // representation.
    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        unimplemented!()
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq> {
        Ok(self)
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple> {
        unimplemented!();
    }

    // Tuple structs look just like sequences in JSON.
    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        unimplemented!();
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
        unimplemented!();
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
}

impl<W: Write> ser::SerializeSeq for &'_ mut Serializer<W> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}
