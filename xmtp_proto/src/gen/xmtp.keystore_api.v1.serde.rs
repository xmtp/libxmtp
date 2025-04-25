// @generated
impl serde::Serialize for GetConversationHmacKeysRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.topics.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.GetConversationHmacKeysRequest", len)?;
        if !self.topics.is_empty() {
            struct_ser.serialize_field("topics", &self.topics)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GetConversationHmacKeysRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "topics",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Topics,
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
                            "topics" => Ok(GeneratedField::Topics),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GetConversationHmacKeysRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.GetConversationHmacKeysRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GetConversationHmacKeysRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut topics__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Topics => {
                            if topics__.is_some() {
                                return Err(serde::de::Error::duplicate_field("topics"));
                            }
                            topics__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(GetConversationHmacKeysRequest {
                    topics: topics__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.GetConversationHmacKeysRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GetConversationHmacKeysResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.hmac_keys.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.GetConversationHmacKeysResponse", len)?;
        if !self.hmac_keys.is_empty() {
            struct_ser.serialize_field("hmacKeys", &self.hmac_keys)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GetConversationHmacKeysResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "hmac_keys",
            "hmacKeys",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            HmacKeys,
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
                            "hmacKeys" | "hmac_keys" => Ok(GeneratedField::HmacKeys),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GetConversationHmacKeysResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.GetConversationHmacKeysResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GetConversationHmacKeysResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut hmac_keys__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::HmacKeys => {
                            if hmac_keys__.is_some() {
                                return Err(serde::de::Error::duplicate_field("hmacKeys"));
                            }
                            hmac_keys__ = Some(
                                map_.next_value::<std::collections::HashMap<_, _>>()?
                            );
                        }
                    }
                }
                Ok(GetConversationHmacKeysResponse {
                    hmac_keys: hmac_keys__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.GetConversationHmacKeysResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for get_conversation_hmac_keys_response::HmacKeyData {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.thirty_day_periods_since_epoch != 0 {
            len += 1;
        }
        if !self.hmac_key.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.GetConversationHmacKeysResponse.HmacKeyData", len)?;
        if self.thirty_day_periods_since_epoch != 0 {
            struct_ser.serialize_field("thirtyDayPeriodsSinceEpoch", &self.thirty_day_periods_since_epoch)?;
        }
        if !self.hmac_key.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("hmacKey", pbjson::private::base64::encode(&self.hmac_key).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for get_conversation_hmac_keys_response::HmacKeyData {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "thirty_day_periods_since_epoch",
            "thirtyDayPeriodsSinceEpoch",
            "hmac_key",
            "hmacKey",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ThirtyDayPeriodsSinceEpoch,
            HmacKey,
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
                            "thirtyDayPeriodsSinceEpoch" | "thirty_day_periods_since_epoch" => Ok(GeneratedField::ThirtyDayPeriodsSinceEpoch),
                            "hmacKey" | "hmac_key" => Ok(GeneratedField::HmacKey),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = get_conversation_hmac_keys_response::HmacKeyData;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.GetConversationHmacKeysResponse.HmacKeyData")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<get_conversation_hmac_keys_response::HmacKeyData, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut thirty_day_periods_since_epoch__ = None;
                let mut hmac_key__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ThirtyDayPeriodsSinceEpoch => {
                            if thirty_day_periods_since_epoch__.is_some() {
                                return Err(serde::de::Error::duplicate_field("thirtyDayPeriodsSinceEpoch"));
                            }
                            thirty_day_periods_since_epoch__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::HmacKey => {
                            if hmac_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("hmacKey"));
                            }
                            hmac_key__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(get_conversation_hmac_keys_response::HmacKeyData {
                    thirty_day_periods_since_epoch: thirty_day_periods_since_epoch__.unwrap_or_default(),
                    hmac_key: hmac_key__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.GetConversationHmacKeysResponse.HmacKeyData", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for get_conversation_hmac_keys_response::HmacKeys {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.values.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.GetConversationHmacKeysResponse.HmacKeys", len)?;
        if !self.values.is_empty() {
            struct_ser.serialize_field("values", &self.values)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for get_conversation_hmac_keys_response::HmacKeys {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "values",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Values,
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
                            "values" => Ok(GeneratedField::Values),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = get_conversation_hmac_keys_response::HmacKeys;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.GetConversationHmacKeysResponse.HmacKeys")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<get_conversation_hmac_keys_response::HmacKeys, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut values__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Values => {
                            if values__.is_some() {
                                return Err(serde::de::Error::duplicate_field("values"));
                            }
                            values__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(get_conversation_hmac_keys_response::HmacKeys {
                    values: values__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.GetConversationHmacKeysResponse.HmacKeys", FIELDS, GeneratedVisitor)
    }
}
