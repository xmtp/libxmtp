// @generated
impl serde::Serialize for ContentTypeSave {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unknown => "CONTENT_TYPE_SAVE_UNKNOWN",
            Self::Text => "CONTENT_TYPE_SAVE_TEXT",
            Self::GroupMembershipChange => "CONTENT_TYPE_SAVE_GROUP_MEMBERSHIP_CHANGE",
            Self::GroupUpdated => "CONTENT_TYPE_SAVE_GROUP_UPDATED",
            Self::Reaction => "CONTENT_TYPE_SAVE_REACTION",
            Self::ReadReceipt => "CONTENT_TYPE_SAVE_READ_RECEIPT",
            Self::Reply => "CONTENT_TYPE_SAVE_REPLY",
            Self::Attachment => "CONTENT_TYPE_SAVE_ATTACHMENT",
            Self::RemoteAttachment => "CONTENT_TYPE_SAVE_REMOTE_ATTACHMENT",
            Self::TransactionReference => "CONTENT_TYPE_SAVE_TRANSACTION_REFERENCE",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ContentTypeSave {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "CONTENT_TYPE_SAVE_UNKNOWN",
            "CONTENT_TYPE_SAVE_TEXT",
            "CONTENT_TYPE_SAVE_GROUP_MEMBERSHIP_CHANGE",
            "CONTENT_TYPE_SAVE_GROUP_UPDATED",
            "CONTENT_TYPE_SAVE_REACTION",
            "CONTENT_TYPE_SAVE_READ_RECEIPT",
            "CONTENT_TYPE_SAVE_REPLY",
            "CONTENT_TYPE_SAVE_ATTACHMENT",
            "CONTENT_TYPE_SAVE_REMOTE_ATTACHMENT",
            "CONTENT_TYPE_SAVE_TRANSACTION_REFERENCE",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ContentTypeSave;

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
                    "CONTENT_TYPE_SAVE_UNKNOWN" => Ok(ContentTypeSave::Unknown),
                    "CONTENT_TYPE_SAVE_TEXT" => Ok(ContentTypeSave::Text),
                    "CONTENT_TYPE_SAVE_GROUP_MEMBERSHIP_CHANGE" => Ok(ContentTypeSave::GroupMembershipChange),
                    "CONTENT_TYPE_SAVE_GROUP_UPDATED" => Ok(ContentTypeSave::GroupUpdated),
                    "CONTENT_TYPE_SAVE_REACTION" => Ok(ContentTypeSave::Reaction),
                    "CONTENT_TYPE_SAVE_READ_RECEIPT" => Ok(ContentTypeSave::ReadReceipt),
                    "CONTENT_TYPE_SAVE_REPLY" => Ok(ContentTypeSave::Reply),
                    "CONTENT_TYPE_SAVE_ATTACHMENT" => Ok(ContentTypeSave::Attachment),
                    "CONTENT_TYPE_SAVE_REMOTE_ATTACHMENT" => Ok(ContentTypeSave::RemoteAttachment),
                    "CONTENT_TYPE_SAVE_TRANSACTION_REFERENCE" => Ok(ContentTypeSave::TransactionReference),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for DeliveryStatusSave {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unpublished => "DELIVERY_STATUS_SAVE_UNPUBLISHED",
            Self::Published => "DELIVERY_STATUS_SAVE_PUBLISHED",
            Self::Failed => "DELIVERY_STATUS_SAVE_FAILED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for DeliveryStatusSave {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "DELIVERY_STATUS_SAVE_UNPUBLISHED",
            "DELIVERY_STATUS_SAVE_PUBLISHED",
            "DELIVERY_STATUS_SAVE_FAILED",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = DeliveryStatusSave;

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
                    "DELIVERY_STATUS_SAVE_UNPUBLISHED" => Ok(DeliveryStatusSave::Unpublished),
                    "DELIVERY_STATUS_SAVE_PUBLISHED" => Ok(DeliveryStatusSave::Published),
                    "DELIVERY_STATUS_SAVE_FAILED" => Ok(DeliveryStatusSave::Failed),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for GroupMessageKindSave {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Application => "GROUP_MESSAGE_KIND_SAVE_APPLICATION",
            Self::MembershipChange => "GROUP_MESSAGE_KIND_SAVE_MEMBERSHIP_CHANGE",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for GroupMessageKindSave {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "GROUP_MESSAGE_KIND_SAVE_APPLICATION",
            "GROUP_MESSAGE_KIND_SAVE_MEMBERSHIP_CHANGE",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GroupMessageKindSave;

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
                    "GROUP_MESSAGE_KIND_SAVE_APPLICATION" => Ok(GroupMessageKindSave::Application),
                    "GROUP_MESSAGE_KIND_SAVE_MEMBERSHIP_CHANGE" => Ok(GroupMessageKindSave::MembershipChange),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for GroupMessageSave {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.id.is_empty() {
            len += 1;
        }
        if !self.group_id.is_empty() {
            len += 1;
        }
        if !self.decrypted_message_bytes.is_empty() {
            len += 1;
        }
        if self.sent_at_ns != 0 {
            len += 1;
        }
        if self.kind != 0 {
            len += 1;
        }
        if !self.sender_installation_id.is_empty() {
            len += 1;
        }
        if !self.sender_inbox_id.is_empty() {
            len += 1;
        }
        if self.delivery_status != 0 {
            len += 1;
        }
        if self.content_type != 0 {
            len += 1;
        }
        if self.version_major != 0 {
            len += 1;
        }
        if self.version_minor != 0 {
            len += 1;
        }
        if !self.authority_id.is_empty() {
            len += 1;
        }
        if self.reference_id.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.message_backup.GroupMessageSave", len)?;
        if !self.id.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("id", pbjson::private::base64::encode(&self.id).as_str())?;
        }
        if !self.group_id.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("groupId", pbjson::private::base64::encode(&self.group_id).as_str())?;
        }
        if !self.decrypted_message_bytes.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("decryptedMessageBytes", pbjson::private::base64::encode(&self.decrypted_message_bytes).as_str())?;
        }
        if self.sent_at_ns != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("sentAtNs", ToString::to_string(&self.sent_at_ns).as_str())?;
        }
        if self.kind != 0 {
            let v = GroupMessageKindSave::try_from(self.kind)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.kind)))?;
            struct_ser.serialize_field("kind", &v)?;
        }
        if !self.sender_installation_id.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("senderInstallationId", pbjson::private::base64::encode(&self.sender_installation_id).as_str())?;
        }
        if !self.sender_inbox_id.is_empty() {
            struct_ser.serialize_field("senderInboxId", &self.sender_inbox_id)?;
        }
        if self.delivery_status != 0 {
            let v = DeliveryStatusSave::try_from(self.delivery_status)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.delivery_status)))?;
            struct_ser.serialize_field("deliveryStatus", &v)?;
        }
        if self.content_type != 0 {
            let v = ContentTypeSave::try_from(self.content_type)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.content_type)))?;
            struct_ser.serialize_field("contentType", &v)?;
        }
        if self.version_major != 0 {
            struct_ser.serialize_field("versionMajor", &self.version_major)?;
        }
        if self.version_minor != 0 {
            struct_ser.serialize_field("versionMinor", &self.version_minor)?;
        }
        if !self.authority_id.is_empty() {
            struct_ser.serialize_field("authorityId", &self.authority_id)?;
        }
        if let Some(v) = self.reference_id.as_ref() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("referenceId", pbjson::private::base64::encode(&v).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GroupMessageSave {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "id",
            "group_id",
            "groupId",
            "decrypted_message_bytes",
            "decryptedMessageBytes",
            "sent_at_ns",
            "sentAtNs",
            "kind",
            "sender_installation_id",
            "senderInstallationId",
            "sender_inbox_id",
            "senderInboxId",
            "delivery_status",
            "deliveryStatus",
            "content_type",
            "contentType",
            "version_major",
            "versionMajor",
            "version_minor",
            "versionMinor",
            "authority_id",
            "authorityId",
            "reference_id",
            "referenceId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Id,
            GroupId,
            DecryptedMessageBytes,
            SentAtNs,
            Kind,
            SenderInstallationId,
            SenderInboxId,
            DeliveryStatus,
            ContentType,
            VersionMajor,
            VersionMinor,
            AuthorityId,
            ReferenceId,
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
                            "id" => Ok(GeneratedField::Id),
                            "groupId" | "group_id" => Ok(GeneratedField::GroupId),
                            "decryptedMessageBytes" | "decrypted_message_bytes" => Ok(GeneratedField::DecryptedMessageBytes),
                            "sentAtNs" | "sent_at_ns" => Ok(GeneratedField::SentAtNs),
                            "kind" => Ok(GeneratedField::Kind),
                            "senderInstallationId" | "sender_installation_id" => Ok(GeneratedField::SenderInstallationId),
                            "senderInboxId" | "sender_inbox_id" => Ok(GeneratedField::SenderInboxId),
                            "deliveryStatus" | "delivery_status" => Ok(GeneratedField::DeliveryStatus),
                            "contentType" | "content_type" => Ok(GeneratedField::ContentType),
                            "versionMajor" | "version_major" => Ok(GeneratedField::VersionMajor),
                            "versionMinor" | "version_minor" => Ok(GeneratedField::VersionMinor),
                            "authorityId" | "authority_id" => Ok(GeneratedField::AuthorityId),
                            "referenceId" | "reference_id" => Ok(GeneratedField::ReferenceId),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GroupMessageSave;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.message_backup.GroupMessageSave")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GroupMessageSave, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut id__ = None;
                let mut group_id__ = None;
                let mut decrypted_message_bytes__ = None;
                let mut sent_at_ns__ = None;
                let mut kind__ = None;
                let mut sender_installation_id__ = None;
                let mut sender_inbox_id__ = None;
                let mut delivery_status__ = None;
                let mut content_type__ = None;
                let mut version_major__ = None;
                let mut version_minor__ = None;
                let mut authority_id__ = None;
                let mut reference_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Id => {
                            if id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("id"));
                            }
                            id__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::GroupId => {
                            if group_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("groupId"));
                            }
                            group_id__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DecryptedMessageBytes => {
                            if decrypted_message_bytes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("decryptedMessageBytes"));
                            }
                            decrypted_message_bytes__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::SentAtNs => {
                            if sent_at_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sentAtNs"));
                            }
                            sent_at_ns__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Kind => {
                            if kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("kind"));
                            }
                            kind__ = Some(map_.next_value::<GroupMessageKindSave>()? as i32);
                        }
                        GeneratedField::SenderInstallationId => {
                            if sender_installation_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("senderInstallationId"));
                            }
                            sender_installation_id__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::SenderInboxId => {
                            if sender_inbox_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("senderInboxId"));
                            }
                            sender_inbox_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::DeliveryStatus => {
                            if delivery_status__.is_some() {
                                return Err(serde::de::Error::duplicate_field("deliveryStatus"));
                            }
                            delivery_status__ = Some(map_.next_value::<DeliveryStatusSave>()? as i32);
                        }
                        GeneratedField::ContentType => {
                            if content_type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("contentType"));
                            }
                            content_type__ = Some(map_.next_value::<ContentTypeSave>()? as i32);
                        }
                        GeneratedField::VersionMajor => {
                            if version_major__.is_some() {
                                return Err(serde::de::Error::duplicate_field("versionMajor"));
                            }
                            version_major__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::VersionMinor => {
                            if version_minor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("versionMinor"));
                            }
                            version_minor__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::AuthorityId => {
                            if authority_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("authorityId"));
                            }
                            authority_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ReferenceId => {
                            if reference_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("referenceId"));
                            }
                            reference_id__ = 
                                map_.next_value::<::std::option::Option<::pbjson::private::BytesDeserialize<_>>>()?.map(|x| x.0)
                            ;
                        }
                    }
                }
                Ok(GroupMessageSave {
                    id: id__.unwrap_or_default(),
                    group_id: group_id__.unwrap_or_default(),
                    decrypted_message_bytes: decrypted_message_bytes__.unwrap_or_default(),
                    sent_at_ns: sent_at_ns__.unwrap_or_default(),
                    kind: kind__.unwrap_or_default(),
                    sender_installation_id: sender_installation_id__.unwrap_or_default(),
                    sender_inbox_id: sender_inbox_id__.unwrap_or_default(),
                    delivery_status: delivery_status__.unwrap_or_default(),
                    content_type: content_type__.unwrap_or_default(),
                    version_major: version_major__.unwrap_or_default(),
                    version_minor: version_minor__.unwrap_or_default(),
                    authority_id: authority_id__.unwrap_or_default(),
                    reference_id: reference_id__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.message_backup.GroupMessageSave", FIELDS, GeneratedVisitor)
    }
}
