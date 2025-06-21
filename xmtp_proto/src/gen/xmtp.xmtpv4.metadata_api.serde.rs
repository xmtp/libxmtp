// @generated
impl serde::Serialize for GetSyncCursorRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("xmtp.xmtpv4.metadata_api.GetSyncCursorRequest", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GetSyncCursorRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
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
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GetSyncCursorRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.metadata_api.GetSyncCursorRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GetSyncCursorRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(GetSyncCursorRequest {
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.metadata_api.GetSyncCursorRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GetSyncCursorResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.latest_sync.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.metadata_api.GetSyncCursorResponse", len)?;
        if let Some(v) = self.latest_sync.as_ref() {
            struct_ser.serialize_field("latest_sync", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GetSyncCursorResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "latest_sync",
            "latestSync",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            LatestSync,
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
                            "latestSync" | "latest_sync" => Ok(GeneratedField::LatestSync),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GetSyncCursorResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.metadata_api.GetSyncCursorResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GetSyncCursorResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut latest_sync__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::LatestSync => {
                            if latest_sync__.is_some() {
                                return Err(serde::de::Error::duplicate_field("latestSync"));
                            }
                            latest_sync__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(GetSyncCursorResponse {
                    latest_sync: latest_sync__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.metadata_api.GetSyncCursorResponse", FIELDS, GeneratedVisitor)
    }
}
