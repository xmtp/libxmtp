// @generated
impl serde::Serialize for PublishClientEnvelopesRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.envelopes.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.payer_api.PublishClientEnvelopesRequest", len)?;
        if !self.envelopes.is_empty() {
            struct_ser.serialize_field("envelopes", &self.envelopes)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for PublishClientEnvelopesRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "envelopes",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Envelopes,
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
                            "envelopes" => Ok(GeneratedField::Envelopes),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = PublishClientEnvelopesRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.payer_api.PublishClientEnvelopesRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<PublishClientEnvelopesRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut envelopes__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Envelopes => {
                            if envelopes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("envelopes"));
                            }
                            envelopes__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(PublishClientEnvelopesRequest {
                    envelopes: envelopes__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.payer_api.PublishClientEnvelopesRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for PublishClientEnvelopesResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.originator_envelopes.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.payer_api.PublishClientEnvelopesResponse", len)?;
        if !self.originator_envelopes.is_empty() {
            struct_ser.serialize_field("originatorEnvelopes", &self.originator_envelopes)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for PublishClientEnvelopesResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "originator_envelopes",
            "originatorEnvelopes",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            OriginatorEnvelopes,
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
                            "originatorEnvelopes" | "originator_envelopes" => Ok(GeneratedField::OriginatorEnvelopes),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = PublishClientEnvelopesResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.payer_api.PublishClientEnvelopesResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<PublishClientEnvelopesResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut originator_envelopes__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::OriginatorEnvelopes => {
                            if originator_envelopes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("originatorEnvelopes"));
                            }
                            originator_envelopes__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(PublishClientEnvelopesResponse {
                    originator_envelopes: originator_envelopes__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.payer_api.PublishClientEnvelopesResponse", FIELDS, GeneratedVisitor)
    }
}
