impl serde::Serialize for BlobSave {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.blob_type != 0 {
            len += 1;
        }
        if !self.blob.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.blob_backup.BlobSave", len)?;
        if self.blob_type != 0 {
            let v = BlobTypeSave::try_from(self.blob_type)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.blob_type)))?;
            struct_ser.serialize_field("blob_type", &v)?;
        }
        if !self.blob.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("blob", pbjson::private::base64::encode(&self.blob).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for BlobSave {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "blob_type",
            "blobType",
            "blob",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            BlobType,
            Blob,
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
                            "blobType" | "blob_type" => Ok(GeneratedField::BlobType),
                            "blob" => Ok(GeneratedField::Blob),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = BlobSave;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.blob_backup.BlobSave")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<BlobSave, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut blob_type__ = None;
                let mut blob__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::BlobType => {
                            if blob_type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("blobType"));
                            }
                            blob_type__ = Some(map_.next_value::<BlobTypeSave>()? as i32);
                        }
                        GeneratedField::Blob => {
                            if blob__.is_some() {
                                return Err(serde::de::Error::duplicate_field("blob"));
                            }
                            blob__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(BlobSave {
                    blob_type: blob_type__.unwrap_or_default(),
                    blob: blob__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.blob_backup.BlobSave", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for BlobTypeSave {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "BLOB_TYPE_SAVE_UNSPECIFIED",
            Self::Snapshot => "BLOB_TYPE_SAVE_SNAPSHOT",
            Self::Keypackage => "BLOB_TYPE_SAVE_KEYPACKAGE",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for BlobTypeSave {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "BLOB_TYPE_SAVE_UNSPECIFIED",
            "BLOB_TYPE_SAVE_SNAPSHOT",
            "BLOB_TYPE_SAVE_KEYPACKAGE",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = BlobTypeSave;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "BLOB_TYPE_SAVE_UNSPECIFIED" => Ok(BlobTypeSave::Unspecified),
                    "BLOB_TYPE_SAVE_SNAPSHOT" => Ok(BlobTypeSave::Snapshot),
                    "BLOB_TYPE_SAVE_KEYPACKAGE" => Ok(BlobTypeSave::Keypackage),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
