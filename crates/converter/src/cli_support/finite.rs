//! A validation-only Serde visitor. JSON itself silently encodes NaN as null.
use serde::ser::*;
use serde::Serialize;

pub(super) struct Finite;
type Result = std::result::Result<(), serde_json::Error>;

macro_rules! scalar {
    ($($name:ident($ty:ty)),* $(,)?) => {
        $(fn $name(self, _: $ty) -> Result { Ok(()) })*
    };
}

impl Serializer for Finite {
    type Ok = ();
    type Error = serde_json::Error;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    scalar! {
        serialize_bool(bool), serialize_i8(i8), serialize_i16(i16),
        serialize_i32(i32), serialize_i64(i64), serialize_i128(i128),
        serialize_u8(u8), serialize_u16(u16), serialize_u32(u32),
        serialize_u64(u64), serialize_u128(u128), serialize_char(char),
        serialize_str(&str), serialize_bytes(&[u8]),
    }
    fn serialize_f32(self, value: f32) -> Result {
        self.serialize_f64(value as f64)
    }
    fn serialize_f64(self, value: f64) -> Result {
        if value.is_finite() {
            Ok(())
        } else {
            Err(Error::custom("nonfinite number in JSON output"))
        }
    }
    fn serialize_none(self) -> Result {
        Ok(())
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result {
        Ok(())
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result {
        Ok(())
    }
    fn serialize_unit_variant(self, _: &'static str, _: u32, _: &'static str) -> Result {
        Ok(())
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(self, _: &'static str, value: &T) -> Result {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        value: &T,
    ) -> Result {
        value.serialize(self)
    }
    fn serialize_seq(self, _: Option<usize>) -> std::result::Result<Self, Self::Error> {
        Ok(self)
    }
    fn serialize_tuple(self, _: usize) -> std::result::Result<Self, Self::Error> {
        Ok(self)
    }
    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> std::result::Result<Self, Self::Error> {
        Ok(self)
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> std::result::Result<Self, Self::Error> {
        Ok(self)
    }
    fn serialize_map(self, _: Option<usize>) -> std::result::Result<Self, Self::Error> {
        Ok(self)
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> std::result::Result<Self, Self::Error> {
        Ok(self)
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> std::result::Result<Self, Self::Error> {
        Ok(self)
    }
}

macro_rules! sequence {
    ($trait:ident, $method:ident) => {
        impl $trait for Finite {
            type Ok = ();
            type Error = serde_json::Error;
            fn $method<T: Serialize + ?Sized>(&mut self, value: &T) -> Result {
                value.serialize(Finite)
            }
            fn end(self) -> Result {
                Ok(())
            }
        }
    };
}
sequence!(SerializeSeq, serialize_element);
sequence!(SerializeTuple, serialize_element);
sequence!(SerializeTupleStruct, serialize_field);
sequence!(SerializeTupleVariant, serialize_field);

impl SerializeMap for Finite {
    type Ok = ();
    type Error = serde_json::Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result {
        key.serialize(Finite)
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result {
        value.serialize(Finite)
    }
    fn end(self) -> Result {
        Ok(())
    }
}

macro_rules! structure {
    ($trait:ident) => {
        impl $trait for Finite {
            type Ok = ();
            type Error = serde_json::Error;
            fn serialize_field<T: Serialize + ?Sized>(
                &mut self,
                _: &'static str,
                value: &T,
            ) -> Result {
                value.serialize(Finite)
            }
            fn end(self) -> Result {
                Ok(())
            }
        }
    };
}
structure!(SerializeStruct);
structure!(SerializeStructVariant);
