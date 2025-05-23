// @generated
impl serde::Serialize for BackupElement {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.element.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.BackupElement", len)?;
        if let Some(v) = self.element.as_ref() {
            match v {
                backup_element::Element::Metadata(v) => {
                    struct_ser.serialize_field("metadata", v)?;
                }
                backup_element::Element::Group(v) => {
                    struct_ser.serialize_field("group", v)?;
                }
                backup_element::Element::GroupMessage(v) => {
                    struct_ser.serialize_field("groupMessage", v)?;
                }
                backup_element::Element::Consent(v) => {
                    struct_ser.serialize_field("consent", v)?;
                }
                backup_element::Element::Event(v) => {
                    struct_ser.serialize_field("event", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for BackupElement {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "metadata",
            "group",
            "group_message",
            "groupMessage",
            "consent",
            "event",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Metadata,
            Group,
            GroupMessage,
            Consent,
            Event,
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
                            "metadata" => Ok(GeneratedField::Metadata),
                            "group" => Ok(GeneratedField::Group),
                            "groupMessage" | "group_message" => Ok(GeneratedField::GroupMessage),
                            "consent" => Ok(GeneratedField::Consent),
                            "event" => Ok(GeneratedField::Event),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = BackupElement;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.BackupElement")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<BackupElement, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut element__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Metadata => {
                            if element__.is_some() {
                                return Err(serde::de::Error::duplicate_field("metadata"));
                            }
                            element__ = map_.next_value::<::std::option::Option<_>>()?.map(backup_element::Element::Metadata)
;
                        }
                        GeneratedField::Group => {
                            if element__.is_some() {
                                return Err(serde::de::Error::duplicate_field("group"));
                            }
                            element__ = map_.next_value::<::std::option::Option<_>>()?.map(backup_element::Element::Group)
;
                        }
                        GeneratedField::GroupMessage => {
                            if element__.is_some() {
                                return Err(serde::de::Error::duplicate_field("groupMessage"));
                            }
                            element__ = map_.next_value::<::std::option::Option<_>>()?.map(backup_element::Element::GroupMessage)
;
                        }
                        GeneratedField::Consent => {
                            if element__.is_some() {
                                return Err(serde::de::Error::duplicate_field("consent"));
                            }
                            element__ = map_.next_value::<::std::option::Option<_>>()?.map(backup_element::Element::Consent)
;
                        }
                        GeneratedField::Event => {
                            if element__.is_some() {
                                return Err(serde::de::Error::duplicate_field("event"));
                            }
                            element__ = map_.next_value::<::std::option::Option<_>>()?.map(backup_element::Element::Event)
;
                        }
                    }
                }
                Ok(BackupElement {
                    element: element__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.BackupElement", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for BackupElementSelection {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "BACKUP_ELEMENT_SELECTION_UNSPECIFIED",
            Self::Messages => "BACKUP_ELEMENT_SELECTION_MESSAGES",
            Self::Consent => "BACKUP_ELEMENT_SELECTION_CONSENT",
            Self::Event => "BACKUP_ELEMENT_SELECTION_EVENT",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for BackupElementSelection {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "BACKUP_ELEMENT_SELECTION_UNSPECIFIED",
            "BACKUP_ELEMENT_SELECTION_MESSAGES",
            "BACKUP_ELEMENT_SELECTION_CONSENT",
            "BACKUP_ELEMENT_SELECTION_EVENT",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = BackupElementSelection;

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
                    "BACKUP_ELEMENT_SELECTION_UNSPECIFIED" => Ok(BackupElementSelection::Unspecified),
                    "BACKUP_ELEMENT_SELECTION_MESSAGES" => Ok(BackupElementSelection::Messages),
                    "BACKUP_ELEMENT_SELECTION_CONSENT" => Ok(BackupElementSelection::Consent),
                    "BACKUP_ELEMENT_SELECTION_EVENT" => Ok(BackupElementSelection::Event),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for BackupMetadataSave {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.elements.is_empty() {
            len += 1;
        }
        if self.exported_at_ns != 0 {
            len += 1;
        }
        if self.start_ns.is_some() {
            len += 1;
        }
        if self.end_ns.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.BackupMetadataSave", len)?;
        if !self.elements.is_empty() {
            let v = self.elements.iter().cloned().map(|v| {
                BackupElementSelection::try_from(v)
                    .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", v)))
                }).collect::<std::result::Result<Vec<_>, _>>()?;
            struct_ser.serialize_field("elements", &v)?;
        }
        if self.exported_at_ns != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("exportedAtNs", ToString::to_string(&self.exported_at_ns).as_str())?;
        }
        if let Some(v) = self.start_ns.as_ref() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("startNs", ToString::to_string(&v).as_str())?;
        }
        if let Some(v) = self.end_ns.as_ref() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("endNs", ToString::to_string(&v).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for BackupMetadataSave {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "elements",
            "exported_at_ns",
            "exportedAtNs",
            "start_ns",
            "startNs",
            "end_ns",
            "endNs",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Elements,
            ExportedAtNs,
            StartNs,
            EndNs,
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
                            "elements" => Ok(GeneratedField::Elements),
                            "exportedAtNs" | "exported_at_ns" => Ok(GeneratedField::ExportedAtNs),
                            "startNs" | "start_ns" => Ok(GeneratedField::StartNs),
                            "endNs" | "end_ns" => Ok(GeneratedField::EndNs),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = BackupMetadataSave;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.BackupMetadataSave")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<BackupMetadataSave, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut elements__ = None;
                let mut exported_at_ns__ = None;
                let mut start_ns__ = None;
                let mut end_ns__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Elements => {
                            if elements__.is_some() {
                                return Err(serde::de::Error::duplicate_field("elements"));
                            }
                            elements__ = Some(map_.next_value::<Vec<BackupElementSelection>>()?.into_iter().map(|x| x as i32).collect());
                        }
                        GeneratedField::ExportedAtNs => {
                            if exported_at_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("exportedAtNs"));
                            }
                            exported_at_ns__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::StartNs => {
                            if start_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("startNs"));
                            }
                            start_ns__ = 
                                map_.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| x.0)
                            ;
                        }
                        GeneratedField::EndNs => {
                            if end_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("endNs"));
                            }
                            end_ns__ = 
                                map_.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| x.0)
                            ;
                        }
                    }
                }
                Ok(BackupMetadataSave {
                    elements: elements__.unwrap_or_default(),
                    exported_at_ns: exported_at_ns__.unwrap_or_default(),
                    start_ns: start_ns__,
                    end_ns: end_ns__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.BackupMetadataSave", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for BackupOptions {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.elements.is_empty() {
            len += 1;
        }
        if self.start_ns.is_some() {
            len += 1;
        }
        if self.end_ns.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.BackupOptions", len)?;
        if !self.elements.is_empty() {
            let v = self.elements.iter().cloned().map(|v| {
                BackupElementSelection::try_from(v)
                    .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", v)))
                }).collect::<std::result::Result<Vec<_>, _>>()?;
            struct_ser.serialize_field("elements", &v)?;
        }
        if let Some(v) = self.start_ns.as_ref() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("startNs", ToString::to_string(&v).as_str())?;
        }
        if let Some(v) = self.end_ns.as_ref() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("endNs", ToString::to_string(&v).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for BackupOptions {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "elements",
            "start_ns",
            "startNs",
            "end_ns",
            "endNs",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Elements,
            StartNs,
            EndNs,
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
                            "elements" => Ok(GeneratedField::Elements),
                            "startNs" | "start_ns" => Ok(GeneratedField::StartNs),
                            "endNs" | "end_ns" => Ok(GeneratedField::EndNs),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = BackupOptions;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.BackupOptions")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<BackupOptions, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut elements__ = None;
                let mut start_ns__ = None;
                let mut end_ns__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Elements => {
                            if elements__.is_some() {
                                return Err(serde::de::Error::duplicate_field("elements"));
                            }
                            elements__ = Some(map_.next_value::<Vec<BackupElementSelection>>()?.into_iter().map(|x| x as i32).collect());
                        }
                        GeneratedField::StartNs => {
                            if start_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("startNs"));
                            }
                            start_ns__ = 
                                map_.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| x.0)
                            ;
                        }
                        GeneratedField::EndNs => {
                            if end_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("endNs"));
                            }
                            end_ns__ = 
                                map_.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| x.0)
                            ;
                        }
                    }
                }
                Ok(BackupOptions {
                    elements: elements__.unwrap_or_default(),
                    start_ns: start_ns__,
                    end_ns: end_ns__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.BackupOptions", FIELDS, GeneratedVisitor)
    }
}
