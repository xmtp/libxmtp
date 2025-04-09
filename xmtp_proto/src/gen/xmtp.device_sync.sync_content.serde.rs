// @generated
impl serde::Serialize for DeviceSyncAcknowledgeKind {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.kind.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.sync_content.DeviceSyncAcknowledgeKind", len)?;
        if let Some(v) = self.kind.as_ref() {
            match v {
                device_sync_acknowledge_kind::Kind::SyncGroupPresence(v) => {
                    struct_ser.serialize_field("syncGroupPresence", v)?;
                }
                device_sync_acknowledge_kind::Kind::Request(v) => {
                    struct_ser.serialize_field("request", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for DeviceSyncAcknowledgeKind {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "sync_group_presence",
            "syncGroupPresence",
            "request",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            SyncGroupPresence,
            Request,
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
                            "syncGroupPresence" | "sync_group_presence" => Ok(GeneratedField::SyncGroupPresence),
                            "request" => Ok(GeneratedField::Request),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = DeviceSyncAcknowledgeKind;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.sync_content.DeviceSyncAcknowledgeKind")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<DeviceSyncAcknowledgeKind, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut kind__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::SyncGroupPresence => {
                            if kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("syncGroupPresence"));
                            }
                            kind__ = map_.next_value::<::std::option::Option<_>>()?.map(device_sync_acknowledge_kind::Kind::SyncGroupPresence);
                        }
                        GeneratedField::Request => {
                            if kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("request"));
                            }
                            kind__ = map_.next_value::<::std::option::Option<_>>()?.map(device_sync_acknowledge_kind::Kind::Request)
;
                        }
                    }
                }
                Ok(DeviceSyncAcknowledgeKind {
                    kind: kind__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.sync_content.DeviceSyncAcknowledgeKind", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for DeviceSyncContent {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.content.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.sync_content.DeviceSyncContent", len)?;
        if let Some(v) = self.content.as_ref() {
            match v {
                device_sync_content::Content::Request(v) => {
                    struct_ser.serialize_field("request", v)?;
                }
                device_sync_content::Content::Payload(v) => {
                    struct_ser.serialize_field("payload", v)?;
                }
                device_sync_content::Content::Acknowledge(v) => {
                    struct_ser.serialize_field("acknowledge", v)?;
                }
                device_sync_content::Content::PreferenceUpdates(v) => {
                    struct_ser.serialize_field("preferenceUpdates", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for DeviceSyncContent {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "request",
            "payload",
            "acknowledge",
            "preference_updates",
            "preferenceUpdates",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Request,
            Payload,
            Acknowledge,
            PreferenceUpdates,
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
                            "request" => Ok(GeneratedField::Request),
                            "payload" => Ok(GeneratedField::Payload),
                            "acknowledge" => Ok(GeneratedField::Acknowledge),
                            "preferenceUpdates" | "preference_updates" => Ok(GeneratedField::PreferenceUpdates),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = DeviceSyncContent;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.sync_content.DeviceSyncContent")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<DeviceSyncContent, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut content__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Request => {
                            if content__.is_some() {
                                return Err(serde::de::Error::duplicate_field("request"));
                            }
                            content__ = map_.next_value::<::std::option::Option<_>>()?.map(device_sync_content::Content::Request)
;
                        }
                        GeneratedField::Payload => {
                            if content__.is_some() {
                                return Err(serde::de::Error::duplicate_field("payload"));
                            }
                            content__ = map_.next_value::<::std::option::Option<_>>()?.map(device_sync_content::Content::Payload)
;
                        }
                        GeneratedField::Acknowledge => {
                            if content__.is_some() {
                                return Err(serde::de::Error::duplicate_field("acknowledge"));
                            }
                            content__ = map_.next_value::<::std::option::Option<_>>()?.map(device_sync_content::Content::Acknowledge)
;
                        }
                        GeneratedField::PreferenceUpdates => {
                            if content__.is_some() {
                                return Err(serde::de::Error::duplicate_field("preferenceUpdates"));
                            }
                            content__ = map_.next_value::<::std::option::Option<_>>()?.map(device_sync_content::Content::PreferenceUpdates)
;
                        }
                    }
                }
                Ok(DeviceSyncContent {
                    content: content__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.sync_content.DeviceSyncContent", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for DeviceSyncHmacKeyUpdate {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.key.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.sync_content.DeviceSyncHmacKeyUpdate", len)?;
        if !self.key.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("key", pbjson::private::base64::encode(&self.key).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for DeviceSyncHmacKeyUpdate {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "key",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Key,
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
                            "key" => Ok(GeneratedField::Key),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = DeviceSyncHmacKeyUpdate;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.sync_content.DeviceSyncHmacKeyUpdate")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<DeviceSyncHmacKeyUpdate, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut key__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Key => {
                            if key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("key"));
                            }
                            key__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(DeviceSyncHmacKeyUpdate {
                    key: key__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.sync_content.DeviceSyncHmacKeyUpdate", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for DeviceSyncPreferenceUpdates {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.updates.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.sync_content.DeviceSyncPreferenceUpdates", len)?;
        if !self.updates.is_empty() {
            struct_ser.serialize_field("updates", &self.updates)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for DeviceSyncPreferenceUpdates {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "updates",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Updates,
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
                            "updates" => Ok(GeneratedField::Updates),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = DeviceSyncPreferenceUpdates;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.sync_content.DeviceSyncPreferenceUpdates")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<DeviceSyncPreferenceUpdates, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut updates__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Updates => {
                            if updates__.is_some() {
                                return Err(serde::de::Error::duplicate_field("updates"));
                            }
                            updates__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(DeviceSyncPreferenceUpdates {
                    updates: updates__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.sync_content.DeviceSyncPreferenceUpdates", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for DeviceSyncRequestAcknowledge {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.request_id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.sync_content.DeviceSyncRequestAcknowledge", len)?;
        if !self.request_id.is_empty() {
            struct_ser.serialize_field("requestId", &self.request_id)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for DeviceSyncRequestAcknowledge {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "request_id",
            "requestId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            RequestId,
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
                            "requestId" | "request_id" => Ok(GeneratedField::RequestId),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = DeviceSyncRequestAcknowledge;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.sync_content.DeviceSyncRequestAcknowledge")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<DeviceSyncRequestAcknowledge, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut request_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::RequestId => {
                            if request_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("requestId"));
                            }
                            request_id__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(DeviceSyncRequestAcknowledge {
                    request_id: request_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.sync_content.DeviceSyncRequestAcknowledge", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for DeviceSyncUserPreferenceUpdate {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.update.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.sync_content.DeviceSyncUserPreferenceUpdate", len)?;
        if let Some(v) = self.update.as_ref() {
            match v {
                device_sync_user_preference_update::Update::ConsentUpdate(v) => {
                    struct_ser.serialize_field("consentUpdate", v)?;
                }
                device_sync_user_preference_update::Update::HmacKeyUpdate(v) => {
                    struct_ser.serialize_field("hmacKeyUpdate", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for DeviceSyncUserPreferenceUpdate {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "consent_update",
            "consentUpdate",
            "hmac_key_update",
            "hmacKeyUpdate",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ConsentUpdate,
            HmacKeyUpdate,
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
                            "consentUpdate" | "consent_update" => Ok(GeneratedField::ConsentUpdate),
                            "hmacKeyUpdate" | "hmac_key_update" => Ok(GeneratedField::HmacKeyUpdate),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = DeviceSyncUserPreferenceUpdate;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.sync_content.DeviceSyncUserPreferenceUpdate")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<DeviceSyncUserPreferenceUpdate, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut update__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ConsentUpdate => {
                            if update__.is_some() {
                                return Err(serde::de::Error::duplicate_field("consentUpdate"));
                            }
                            update__ = map_.next_value::<::std::option::Option<_>>()?.map(device_sync_user_preference_update::Update::ConsentUpdate)
;
                        }
                        GeneratedField::HmacKeyUpdate => {
                            if update__.is_some() {
                                return Err(serde::de::Error::duplicate_field("hmacKeyUpdate"));
                            }
                            update__ = map_.next_value::<::std::option::Option<_>>()?.map(device_sync_user_preference_update::Update::HmacKeyUpdate)
;
                        }
                    }
                }
                Ok(DeviceSyncUserPreferenceUpdate {
                    update: update__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.sync_content.DeviceSyncUserPreferenceUpdate", FIELDS, GeneratedVisitor)
    }
}
