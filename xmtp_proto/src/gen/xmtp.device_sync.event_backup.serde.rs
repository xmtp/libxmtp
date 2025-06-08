// @generated
impl serde::Serialize for EventLevelSave {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "EVENT_LEVEL_SAVE_UNSPECIFIED",
            Self::None => "EVENT_LEVEL_SAVE_NONE",
            Self::Success => "EVENT_LEVEL_SAVE_SUCCESS",
            Self::Warn => "EVENT_LEVEL_SAVE_WARN",
            Self::Error => "EVENT_LEVEL_SAVE_ERROR",
            Self::Fault => "EVENT_LEVEL_SAVE_FAULT",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for EventLevelSave {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "EVENT_LEVEL_SAVE_UNSPECIFIED",
            "EVENT_LEVEL_SAVE_NONE",
            "EVENT_LEVEL_SAVE_SUCCESS",
            "EVENT_LEVEL_SAVE_WARN",
            "EVENT_LEVEL_SAVE_ERROR",
            "EVENT_LEVEL_SAVE_FAULT",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EventLevelSave;

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
                    "EVENT_LEVEL_SAVE_UNSPECIFIED" => Ok(EventLevelSave::Unspecified),
                    "EVENT_LEVEL_SAVE_NONE" => Ok(EventLevelSave::None),
                    "EVENT_LEVEL_SAVE_SUCCESS" => Ok(EventLevelSave::Success),
                    "EVENT_LEVEL_SAVE_WARN" => Ok(EventLevelSave::Warn),
                    "EVENT_LEVEL_SAVE_ERROR" => Ok(EventLevelSave::Error),
                    "EVENT_LEVEL_SAVE_FAULT" => Ok(EventLevelSave::Fault),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for EventSave {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.created_at_ns != 0 {
            len += 1;
        }
        if !self.event.is_empty() {
            len += 1;
        }
        if !self.details.is_empty() {
            len += 1;
        }
        if self.group_id.is_some() {
            len += 1;
        }
        if self.level != 0 {
            len += 1;
        }
        if self.icon.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.event_backup.EventSave", len)?;
        if self.created_at_ns != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("createdAtNs", ToString::to_string(&self.created_at_ns).as_str())?;
        }
        if !self.event.is_empty() {
            struct_ser.serialize_field("event", &self.event)?;
        }
        if !self.details.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("details", pbjson::private::base64::encode(&self.details).as_str())?;
        }
        if let Some(v) = self.group_id.as_ref() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("groupId", pbjson::private::base64::encode(&v).as_str())?;
        }
        if self.level != 0 {
            let v = EventLevelSave::try_from(self.level)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.level)))?;
            struct_ser.serialize_field("level", &v)?;
        }
        if let Some(v) = self.icon.as_ref() {
            struct_ser.serialize_field("icon", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EventSave {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "created_at_ns",
            "createdAtNs",
            "event",
            "details",
            "group_id",
            "groupId",
            "level",
            "icon",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            CreatedAtNs,
            Event,
            Details,
            GroupId,
            Level,
            Icon,
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
                            "createdAtNs" | "created_at_ns" => Ok(GeneratedField::CreatedAtNs),
                            "event" => Ok(GeneratedField::Event),
                            "details" => Ok(GeneratedField::Details),
                            "groupId" | "group_id" => Ok(GeneratedField::GroupId),
                            "level" => Ok(GeneratedField::Level),
                            "icon" => Ok(GeneratedField::Icon),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EventSave;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.event_backup.EventSave")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EventSave, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut created_at_ns__ = None;
                let mut event__ = None;
                let mut details__ = None;
                let mut group_id__ = None;
                let mut level__ = None;
                let mut icon__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::CreatedAtNs => {
                            if created_at_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createdAtNs"));
                            }
                            created_at_ns__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Event => {
                            if event__.is_some() {
                                return Err(serde::de::Error::duplicate_field("event"));
                            }
                            event__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Details => {
                            if details__.is_some() {
                                return Err(serde::de::Error::duplicate_field("details"));
                            }
                            details__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::GroupId => {
                            if group_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("groupId"));
                            }
                            group_id__ = 
                                map_.next_value::<::std::option::Option<::pbjson::private::BytesDeserialize<_>>>()?.map(|x| x.0)
                            ;
                        }
                        GeneratedField::Level => {
                            if level__.is_some() {
                                return Err(serde::de::Error::duplicate_field("level"));
                            }
                            level__ = Some(map_.next_value::<EventLevelSave>()? as i32);
                        }
                        GeneratedField::Icon => {
                            if icon__.is_some() {
                                return Err(serde::de::Error::duplicate_field("icon"));
                            }
                            icon__ = map_.next_value()?;
                        }
                    }
                }
                Ok(EventSave {
                    created_at_ns: created_at_ns__.unwrap_or_default(),
                    event: event__.unwrap_or_default(),
                    details: details__.unwrap_or_default(),
                    group_id: group_id__,
                    level: level__.unwrap_or_default(),
                    icon: icon__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.event_backup.EventSave", FIELDS, GeneratedVisitor)
    }
}
