impl serde::Serialize for FetchD14nCutoverResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.timestamp_ns != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.migration.api.v1.FetchD14nCutoverResponse", len)?;
        if self.timestamp_ns != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("timestamp_ns", ToString::to_string(&self.timestamp_ns).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for FetchD14nCutoverResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "timestamp_ns",
            "timestampNs",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            TimestampNs,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "timestampNs" | "timestamp_ns" => Ok(GeneratedField::TimestampNs),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = FetchD14nCutoverResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.migration.api.v1.FetchD14nCutoverResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<FetchD14nCutoverResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut timestamp_ns__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::TimestampNs => {
                            if timestamp_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("timestampNs"));
                            }
                            timestamp_ns__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(FetchD14nCutoverResponse {
                    timestamp_ns: timestamp_ns__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.migration.api.v1.FetchD14nCutoverResponse", FIELDS, GeneratedVisitor)
    }
}
