// @generated
impl serde::Serialize for DeviceSyncAcknowledge {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.content.DeviceSyncAcknowledge", len)?;
        if !self.request_id.is_empty() {
            struct_ser.serialize_field("requestId", &self.request_id)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for DeviceSyncAcknowledge {
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
            type Value = DeviceSyncAcknowledge;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.content.DeviceSyncAcknowledge")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<DeviceSyncAcknowledge, V::Error>
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
                Ok(DeviceSyncAcknowledge {
                    request_id: request_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.content.DeviceSyncAcknowledge", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.content.DeviceSyncContent", len)?;
        if let Some(v) = self.content.as_ref() {
            match v {
                device_sync_content::Content::Request(v) => {
                    struct_ser.serialize_field("request", v)?;
                }
                device_sync_content::Content::Acknowledge(v) => {
                    struct_ser.serialize_field("acknowledge", v)?;
                }
                device_sync_content::Content::Reply(v) => {
                    struct_ser.serialize_field("reply", v)?;
                }
                device_sync_content::Content::PreferenceUpdates(v) => {
                    struct_ser.serialize_field("preferenceUpdates", v)?;
                }
                device_sync_content::Content::SyncGroupContest(v) => {
                    struct_ser.serialize_field("syncGroupContest", v)?;
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
            "acknowledge",
            "reply",
            "preference_updates",
            "preferenceUpdates",
            "sync_group_contest",
            "syncGroupContest",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Request,
            Acknowledge,
            Reply,
            PreferenceUpdates,
            SyncGroupContest,
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
                            "acknowledge" => Ok(GeneratedField::Acknowledge),
                            "reply" => Ok(GeneratedField::Reply),
                            "preferenceUpdates" | "preference_updates" => Ok(GeneratedField::PreferenceUpdates),
                            "syncGroupContest" | "sync_group_contest" => Ok(GeneratedField::SyncGroupContest),
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
                formatter.write_str("struct xmtp.device_sync.content.DeviceSyncContent")
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
                        GeneratedField::Acknowledge => {
                            if content__.is_some() {
                                return Err(serde::de::Error::duplicate_field("acknowledge"));
                            }
                            content__ = map_.next_value::<::std::option::Option<_>>()?.map(device_sync_content::Content::Acknowledge)
;
                        }
                        GeneratedField::Reply => {
                            if content__.is_some() {
                                return Err(serde::de::Error::duplicate_field("reply"));
                            }
                            content__ = map_.next_value::<::std::option::Option<_>>()?.map(device_sync_content::Content::Reply)
;
                        }
                        GeneratedField::PreferenceUpdates => {
                            if content__.is_some() {
                                return Err(serde::de::Error::duplicate_field("preferenceUpdates"));
                            }
                            content__ = map_.next_value::<::std::option::Option<_>>()?.map(device_sync_content::Content::PreferenceUpdates)
;
                        }
                        GeneratedField::SyncGroupContest => {
                            if content__.is_some() {
                                return Err(serde::de::Error::duplicate_field("syncGroupContest"));
                            }
                            content__ = map_.next_value::<::std::option::Option<_>>()?.map(device_sync_content::Content::SyncGroupContest)
;
                        }
                    }
                }
                Ok(DeviceSyncContent {
                    content: content__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.content.DeviceSyncContent", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for HmacKeyUpdate {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.content.HmacKeyUpdate", len)?;
        if !self.key.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("key", pbjson::private::base64::encode(&self.key).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for HmacKeyUpdate {
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
            type Value = HmacKeyUpdate;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.content.HmacKeyUpdate")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<HmacKeyUpdate, V::Error>
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
                Ok(HmacKeyUpdate {
                    key: key__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.content.HmacKeyUpdate", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for PreferenceUpdates {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.content.PreferenceUpdates", len)?;
        if !self.updates.is_empty() {
            struct_ser.serialize_field("updates", &self.updates)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for PreferenceUpdates {
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
            type Value = PreferenceUpdates;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.content.PreferenceUpdates")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<PreferenceUpdates, V::Error>
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
                Ok(PreferenceUpdates {
                    updates: updates__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.content.PreferenceUpdates", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SyncGroupContest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.group_id.is_empty() {
            len += 1;
        }
        if self.oldest_message_timestamp != 0 {
            len += 1;
        }
        if self.cited_message_id.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.content.SyncGroupContest", len)?;
        if !self.group_id.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("groupId", pbjson::private::base64::encode(&self.group_id).as_str())?;
        }
        if self.oldest_message_timestamp != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("oldestMessageTimestamp", ToString::to_string(&self.oldest_message_timestamp).as_str())?;
        }
        if let Some(v) = self.cited_message_id.as_ref() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("citedMessageId", pbjson::private::base64::encode(&v).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SyncGroupContest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "group_id",
            "groupId",
            "oldest_message_timestamp",
            "oldestMessageTimestamp",
            "cited_message_id",
            "citedMessageId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            GroupId,
            OldestMessageTimestamp,
            CitedMessageId,
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
                            "groupId" | "group_id" => Ok(GeneratedField::GroupId),
                            "oldestMessageTimestamp" | "oldest_message_timestamp" => Ok(GeneratedField::OldestMessageTimestamp),
                            "citedMessageId" | "cited_message_id" => Ok(GeneratedField::CitedMessageId),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SyncGroupContest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.content.SyncGroupContest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SyncGroupContest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut group_id__ = None;
                let mut oldest_message_timestamp__ = None;
                let mut cited_message_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::GroupId => {
                            if group_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("groupId"));
                            }
                            group_id__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::OldestMessageTimestamp => {
                            if oldest_message_timestamp__.is_some() {
                                return Err(serde::de::Error::duplicate_field("oldestMessageTimestamp"));
                            }
                            oldest_message_timestamp__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::CitedMessageId => {
                            if cited_message_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("citedMessageId"));
                            }
                            cited_message_id__ = 
                                map_.next_value::<::std::option::Option<::pbjson::private::BytesDeserialize<_>>>()?.map(|x| x.0)
                            ;
                        }
                    }
                }
                Ok(SyncGroupContest {
                    group_id: group_id__.unwrap_or_default(),
                    oldest_message_timestamp: oldest_message_timestamp__.unwrap_or_default(),
                    cited_message_id: cited_message_id__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.content.SyncGroupContest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for UserPreferenceUpdate {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.content.UserPreferenceUpdate", len)?;
        if let Some(v) = self.update.as_ref() {
            match v {
                user_preference_update::Update::Consent(v) => {
                    struct_ser.serialize_field("consent", v)?;
                }
                user_preference_update::Update::Hmac(v) => {
                    struct_ser.serialize_field("hmac", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for UserPreferenceUpdate {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "consent",
            "hmac",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Consent,
            Hmac,
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
                            "consent" => Ok(GeneratedField::Consent),
                            "hmac" => Ok(GeneratedField::Hmac),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = UserPreferenceUpdate;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.content.UserPreferenceUpdate")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<UserPreferenceUpdate, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut update__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Consent => {
                            if update__.is_some() {
                                return Err(serde::de::Error::duplicate_field("consent"));
                            }
                            update__ = map_.next_value::<::std::option::Option<_>>()?.map(user_preference_update::Update::Consent)
;
                        }
                        GeneratedField::Hmac => {
                            if update__.is_some() {
                                return Err(serde::de::Error::duplicate_field("hmac"));
                            }
                            update__ = map_.next_value::<::std::option::Option<_>>()?.map(user_preference_update::Update::Hmac)
;
                        }
                    }
                }
                Ok(UserPreferenceUpdate {
                    update: update__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.content.UserPreferenceUpdate", FIELDS, GeneratedVisitor)
    }
}
