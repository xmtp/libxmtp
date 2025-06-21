// @generated
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
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.event_backup.EventSave", len)?;
        if self.created_at_ns != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("created_at_ns", ToString::to_string(&self.created_at_ns).as_str())?;
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
            struct_ser.serialize_field("group_id", pbjson::private::base64::encode(&v).as_str())?;
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
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            CreatedAtNs,
            Event,
            Details,
            GroupId,
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
                            "createdAtNs" | "created_at_ns" => Ok(GeneratedField::CreatedAtNs),
                            "event" => Ok(GeneratedField::Event),
                            "details" => Ok(GeneratedField::Details),
                            "groupId" | "group_id" => Ok(GeneratedField::GroupId),
                            _ => Ok(GeneratedField::__SkipField__),
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EventSave {
                    created_at_ns: created_at_ns__.unwrap_or_default(),
                    event: event__.unwrap_or_default(),
                    details: details__.unwrap_or_default(),
                    group_id: group_id__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.event_backup.EventSave", FIELDS, GeneratedVisitor)
    }
}
