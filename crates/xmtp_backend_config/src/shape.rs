//! Decode a TOML document by the shape each key's type names.
//!
//! `toml::Value` decodes a struct from an array by position, so `limits = [7]`
//! would set the first key of `[limits]`, and it decodes an enum from a table
//! with one key, so `log_level = { debug = {} }` would read as `debug`. Every
//! table with fixed keys in the configuration is a struct and every key with a
//! fixed list of values is a unit-variant enum, so `Shaped` refuses both
//! shapes at every depth and otherwise decodes as `toml::Value` does.

use serde::de::{
    self, Deserializer, IntoDeserializer, Unexpected, Visitor,
    value::{MapDeserializer, SeqDeserializer},
};

/// A `toml::Value` decoded with the shape checks above.
// implements: CONF-087
pub(crate) struct Shaped(pub(crate) toml::Value);

type Error = toml::de::Error;

/// The shape of a refused value. It never holds the value, which can be a
/// secret.
fn unexpected(value: &toml::Value) -> Unexpected<'static> {
    match value {
        toml::Value::Array(_) => Unexpected::Seq,
        toml::Value::Table(_) => Unexpected::Map,
        other => Unexpected::Other(other.type_str()),
    }
}

impl<'de> IntoDeserializer<'de, Error> for Shaped {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self {
        self
    }
}

impl<'de> Deserializer<'de> for Shaped {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            toml::Value::Array(values) => {
                let mut seq = SeqDeserializer::new(values.into_iter().map(Shaped));
                let decoded = visitor.visit_seq(&mut seq)?;
                seq.end()?;
                Ok(decoded)
            }
            toml::Value::Table(table) => {
                let mut map = MapDeserializer::new(
                    table.into_iter().map(|(key, value)| (key, Shaped(value))),
                );
                let decoded = visitor.visit_map(&mut map)?;
                map.end()?;
                Ok(decoded)
            }
            scalar => scalar.deserialize_any(visitor),
        }
    }

    /// A table with fixed keys is decoded from a table only.
    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        match self.0 {
            toml::Value::Table(_) => self.deserialize_any(visitor),
            other => Err(de::Error::invalid_type(unexpected(&other), &visitor)),
        }
    }

    /// A key with a fixed list of values is decoded from a string only.
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        match self.0 {
            toml::Value::String(variant) => {
                visitor.visit_enum(IntoDeserializer::<Error>::into_deserializer(variant))
            }
            other => Err(de::Error::invalid_type(unexpected(&other), &visitor)),
        }
    }

    // A present key is `Some`; an absent one never reaches the decoder.
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_some(self)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor.visit_newtype_struct(self)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string
        bytes byte_buf unit unit_struct seq tuple tuple_struct map identifier
        ignored_any
    }
}
