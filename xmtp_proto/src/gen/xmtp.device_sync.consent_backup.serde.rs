// @generated
impl serde::Serialize for ConsentSave {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.entity_type != 0 {
            len += 1;
        }
        if self.state != 0 {
            len += 1;
        }
        if !self.entity.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.consent_backup.ConsentSave", len)?;
        if self.entity_type != 0 {
            let v = ConsentTypeSave::try_from(self.entity_type)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.entity_type)))?;
            struct_ser.serialize_field("entityType", &v)?;
        }
        if self.state != 0 {
            let v = ConsentStateSave::try_from(self.state)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.state)))?;
            struct_ser.serialize_field("state", &v)?;
        }
        if !self.entity.is_empty() {
            struct_ser.serialize_field("entity", &self.entity)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ConsentSave {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "entity_type",
            "entityType",
            "state",
            "entity",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            EntityType,
            State,
            Entity,
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
                            "entityType" | "entity_type" => Ok(GeneratedField::EntityType),
                            "state" => Ok(GeneratedField::State),
                            "entity" => Ok(GeneratedField::Entity),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ConsentSave;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.consent_backup.ConsentSave")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ConsentSave, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut entity_type__ = None;
                let mut state__ = None;
                let mut entity__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::EntityType => {
                            if entity_type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("entityType"));
                            }
                            entity_type__ = Some(map_.next_value::<ConsentTypeSave>()? as i32);
                        }
                        GeneratedField::State => {
                            if state__.is_some() {
                                return Err(serde::de::Error::duplicate_field("state"));
                            }
                            state__ = Some(map_.next_value::<ConsentStateSave>()? as i32);
                        }
                        GeneratedField::Entity => {
                            if entity__.is_some() {
                                return Err(serde::de::Error::duplicate_field("entity"));
                            }
                            entity__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(ConsentSave {
                    entity_type: entity_type__.unwrap_or_default(),
                    state: state__.unwrap_or_default(),
                    entity: entity__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.consent_backup.ConsentSave", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ConsentStateSave {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::ConseNtStateSaveUnspecified => "CONSENt_STATE_SAVE_UNSPECIFIED",
            Self::Unknown => "CONSENT_STATE_SAVE_UNKNOWN",
            Self::Allowed => "CONSENT_STATE_SAVE_ALLOWED",
            Self::Denied => "CONSENT_STATE_SAVE_DENIED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ConsentStateSave {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "CONSENt_STATE_SAVE_UNSPECIFIED",
            "CONSENT_STATE_SAVE_UNKNOWN",
            "CONSENT_STATE_SAVE_ALLOWED",
            "CONSENT_STATE_SAVE_DENIED",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ConsentStateSave;

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
                    "CONSENt_STATE_SAVE_UNSPECIFIED" => Ok(ConsentStateSave::ConseNtStateSaveUnspecified),
                    "CONSENT_STATE_SAVE_UNKNOWN" => Ok(ConsentStateSave::Unknown),
                    "CONSENT_STATE_SAVE_ALLOWED" => Ok(ConsentStateSave::Allowed),
                    "CONSENT_STATE_SAVE_DENIED" => Ok(ConsentStateSave::Denied),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ConsentTypeSave {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "CONSENT_TYPE_SAVE_UNSPECIFIED",
            Self::ConversationId => "CONSENT_TYPE_SAVE_CONVERSATION_ID",
            Self::InboxId => "CONSENT_TYPE_SAVE_INBOX_ID",
            Self::Address => "CONSENT_TYPE_SAVE_ADDRESS",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ConsentTypeSave {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "CONSENT_TYPE_SAVE_UNSPECIFIED",
            "CONSENT_TYPE_SAVE_CONVERSATION_ID",
            "CONSENT_TYPE_SAVE_INBOX_ID",
            "CONSENT_TYPE_SAVE_ADDRESS",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ConsentTypeSave;

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
                    "CONSENT_TYPE_SAVE_UNSPECIFIED" => Ok(ConsentTypeSave::Unspecified),
                    "CONSENT_TYPE_SAVE_CONVERSATION_ID" => Ok(ConsentTypeSave::ConversationId),
                    "CONSENT_TYPE_SAVE_INBOX_ID" => Ok(ConsentTypeSave::InboxId),
                    "CONSENT_TYPE_SAVE_ADDRESS" => Ok(ConsentTypeSave::Address),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
