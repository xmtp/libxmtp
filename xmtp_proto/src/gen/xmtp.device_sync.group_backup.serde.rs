// @generated
impl serde::Serialize for ConversationTypeSave {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "CONVERSATION_TYPE_SAVE_UNSPECIFIED",
            Self::Group => "CONVERSATION_TYPE_SAVE_GROUP",
            Self::Dm => "CONVERSATION_TYPE_SAVE_DM",
            Self::Sync => "CONVERSATION_TYPE_SAVE_SYNC",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ConversationTypeSave {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "CONVERSATION_TYPE_SAVE_UNSPECIFIED",
            "CONVERSATION_TYPE_SAVE_GROUP",
            "CONVERSATION_TYPE_SAVE_DM",
            "CONVERSATION_TYPE_SAVE_SYNC",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ConversationTypeSave;

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
                    "CONVERSATION_TYPE_SAVE_UNSPECIFIED" => Ok(ConversationTypeSave::Unspecified),
                    "CONVERSATION_TYPE_SAVE_GROUP" => Ok(ConversationTypeSave::Group),
                    "CONVERSATION_TYPE_SAVE_DM" => Ok(ConversationTypeSave::Dm),
                    "CONVERSATION_TYPE_SAVE_SYNC" => Ok(ConversationTypeSave::Sync),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for GroupMembershipStateSave {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "GROUP_MEMBERSHIP_STATE_SAVE_UNSPECIFIED",
            Self::Allowed => "GROUP_MEMBERSHIP_STATE_SAVE_ALLOWED",
            Self::Rejected => "GROUP_MEMBERSHIP_STATE_SAVE_REJECTED",
            Self::Pending => "GROUP_MEMBERSHIP_STATE_SAVE_PENDING",
            Self::Restored => "GROUP_MEMBERSHIP_STATE_SAVE_RESTORED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for GroupMembershipStateSave {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "GROUP_MEMBERSHIP_STATE_SAVE_UNSPECIFIED",
            "GROUP_MEMBERSHIP_STATE_SAVE_ALLOWED",
            "GROUP_MEMBERSHIP_STATE_SAVE_REJECTED",
            "GROUP_MEMBERSHIP_STATE_SAVE_PENDING",
            "GROUP_MEMBERSHIP_STATE_SAVE_RESTORED",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GroupMembershipStateSave;

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
                    "GROUP_MEMBERSHIP_STATE_SAVE_UNSPECIFIED" => Ok(GroupMembershipStateSave::Unspecified),
                    "GROUP_MEMBERSHIP_STATE_SAVE_ALLOWED" => Ok(GroupMembershipStateSave::Allowed),
                    "GROUP_MEMBERSHIP_STATE_SAVE_REJECTED" => Ok(GroupMembershipStateSave::Rejected),
                    "GROUP_MEMBERSHIP_STATE_SAVE_PENDING" => Ok(GroupMembershipStateSave::Pending),
                    "GROUP_MEMBERSHIP_STATE_SAVE_RESTORED" => Ok(GroupMembershipStateSave::Restored),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for GroupSave {
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
        if self.created_at_ns != 0 {
            len += 1;
        }
        if self.membership_state != 0 {
            len += 1;
        }
        if self.installations_last_checked != 0 {
            len += 1;
        }
        if !self.added_by_inbox_id.is_empty() {
            len += 1;
        }
        if self.welcome_id.is_some() {
            len += 1;
        }
        if self.rotated_at_ns != 0 {
            len += 1;
        }
        if self.conversation_type != 0 {
            len += 1;
        }
        if self.dm_id.is_some() {
            len += 1;
        }
        if self.last_message_ns.is_some() {
            len += 1;
        }
        if self.message_disappear_from_ns.is_some() {
            len += 1;
        }
        if self.message_disappear_in_ns.is_some() {
            len += 1;
        }
        if self.metadata.is_some() {
            len += 1;
        }
        if self.mutable_metadata.is_some() {
            len += 1;
        }
        if self.paused_for_version.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.group_backup.GroupSave", len)?;
        if !self.id.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("id", pbjson::private::base64::encode(&self.id).as_str())?;
        }
        if self.created_at_ns != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("createdAtNs", ToString::to_string(&self.created_at_ns).as_str())?;
        }
        if self.membership_state != 0 {
            let v = GroupMembershipStateSave::try_from(self.membership_state)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.membership_state)))?;
            struct_ser.serialize_field("membershipState", &v)?;
        }
        if self.installations_last_checked != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("installationsLastChecked", ToString::to_string(&self.installations_last_checked).as_str())?;
        }
        if !self.added_by_inbox_id.is_empty() {
            struct_ser.serialize_field("addedByInboxId", &self.added_by_inbox_id)?;
        }
        if let Some(v) = self.welcome_id.as_ref() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("welcomeId", ToString::to_string(&v).as_str())?;
        }
        if self.rotated_at_ns != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("rotatedAtNs", ToString::to_string(&self.rotated_at_ns).as_str())?;
        }
        if self.conversation_type != 0 {
            let v = ConversationTypeSave::try_from(self.conversation_type)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.conversation_type)))?;
            struct_ser.serialize_field("conversationType", &v)?;
        }
        if let Some(v) = self.dm_id.as_ref() {
            struct_ser.serialize_field("dmId", v)?;
        }
        if let Some(v) = self.last_message_ns.as_ref() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("lastMessageNs", ToString::to_string(&v).as_str())?;
        }
        if let Some(v) = self.message_disappear_from_ns.as_ref() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("messageDisappearFromNs", ToString::to_string(&v).as_str())?;
        }
        if let Some(v) = self.message_disappear_in_ns.as_ref() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("messageDisappearInNs", ToString::to_string(&v).as_str())?;
        }
        if let Some(v) = self.metadata.as_ref() {
            struct_ser.serialize_field("metadata", v)?;
        }
        if let Some(v) = self.mutable_metadata.as_ref() {
            struct_ser.serialize_field("mutableMetadata", v)?;
        }
        if let Some(v) = self.paused_for_version.as_ref() {
            struct_ser.serialize_field("pausedForVersion", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GroupSave {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "id",
            "created_at_ns",
            "createdAtNs",
            "membership_state",
            "membershipState",
            "installations_last_checked",
            "installationsLastChecked",
            "added_by_inbox_id",
            "addedByInboxId",
            "welcome_id",
            "welcomeId",
            "rotated_at_ns",
            "rotatedAtNs",
            "conversation_type",
            "conversationType",
            "dm_id",
            "dmId",
            "last_message_ns",
            "lastMessageNs",
            "message_disappear_from_ns",
            "messageDisappearFromNs",
            "message_disappear_in_ns",
            "messageDisappearInNs",
            "metadata",
            "mutable_metadata",
            "mutableMetadata",
            "paused_for_version",
            "pausedForVersion",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Id,
            CreatedAtNs,
            MembershipState,
            InstallationsLastChecked,
            AddedByInboxId,
            WelcomeId,
            RotatedAtNs,
            ConversationType,
            DmId,
            LastMessageNs,
            MessageDisappearFromNs,
            MessageDisappearInNs,
            Metadata,
            MutableMetadata,
            PausedForVersion,
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
                            "createdAtNs" | "created_at_ns" => Ok(GeneratedField::CreatedAtNs),
                            "membershipState" | "membership_state" => Ok(GeneratedField::MembershipState),
                            "installationsLastChecked" | "installations_last_checked" => Ok(GeneratedField::InstallationsLastChecked),
                            "addedByInboxId" | "added_by_inbox_id" => Ok(GeneratedField::AddedByInboxId),
                            "welcomeId" | "welcome_id" => Ok(GeneratedField::WelcomeId),
                            "rotatedAtNs" | "rotated_at_ns" => Ok(GeneratedField::RotatedAtNs),
                            "conversationType" | "conversation_type" => Ok(GeneratedField::ConversationType),
                            "dmId" | "dm_id" => Ok(GeneratedField::DmId),
                            "lastMessageNs" | "last_message_ns" => Ok(GeneratedField::LastMessageNs),
                            "messageDisappearFromNs" | "message_disappear_from_ns" => Ok(GeneratedField::MessageDisappearFromNs),
                            "messageDisappearInNs" | "message_disappear_in_ns" => Ok(GeneratedField::MessageDisappearInNs),
                            "metadata" => Ok(GeneratedField::Metadata),
                            "mutableMetadata" | "mutable_metadata" => Ok(GeneratedField::MutableMetadata),
                            "pausedForVersion" | "paused_for_version" => Ok(GeneratedField::PausedForVersion),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GroupSave;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.group_backup.GroupSave")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GroupSave, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut id__ = None;
                let mut created_at_ns__ = None;
                let mut membership_state__ = None;
                let mut installations_last_checked__ = None;
                let mut added_by_inbox_id__ = None;
                let mut welcome_id__ = None;
                let mut rotated_at_ns__ = None;
                let mut conversation_type__ = None;
                let mut dm_id__ = None;
                let mut last_message_ns__ = None;
                let mut message_disappear_from_ns__ = None;
                let mut message_disappear_in_ns__ = None;
                let mut metadata__ = None;
                let mut mutable_metadata__ = None;
                let mut paused_for_version__ = None;
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
                        GeneratedField::CreatedAtNs => {
                            if created_at_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createdAtNs"));
                            }
                            created_at_ns__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::MembershipState => {
                            if membership_state__.is_some() {
                                return Err(serde::de::Error::duplicate_field("membershipState"));
                            }
                            membership_state__ = Some(map_.next_value::<GroupMembershipStateSave>()? as i32);
                        }
                        GeneratedField::InstallationsLastChecked => {
                            if installations_last_checked__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationsLastChecked"));
                            }
                            installations_last_checked__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::AddedByInboxId => {
                            if added_by_inbox_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("addedByInboxId"));
                            }
                            added_by_inbox_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::WelcomeId => {
                            if welcome_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("welcomeId"));
                            }
                            welcome_id__ = 
                                map_.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| x.0)
                            ;
                        }
                        GeneratedField::RotatedAtNs => {
                            if rotated_at_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("rotatedAtNs"));
                            }
                            rotated_at_ns__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ConversationType => {
                            if conversation_type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("conversationType"));
                            }
                            conversation_type__ = Some(map_.next_value::<ConversationTypeSave>()? as i32);
                        }
                        GeneratedField::DmId => {
                            if dm_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dmId"));
                            }
                            dm_id__ = map_.next_value()?;
                        }
                        GeneratedField::LastMessageNs => {
                            if last_message_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("lastMessageNs"));
                            }
                            last_message_ns__ = 
                                map_.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| x.0)
                            ;
                        }
                        GeneratedField::MessageDisappearFromNs => {
                            if message_disappear_from_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("messageDisappearFromNs"));
                            }
                            message_disappear_from_ns__ = 
                                map_.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| x.0)
                            ;
                        }
                        GeneratedField::MessageDisappearInNs => {
                            if message_disappear_in_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("messageDisappearInNs"));
                            }
                            message_disappear_in_ns__ = 
                                map_.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| x.0)
                            ;
                        }
                        GeneratedField::Metadata => {
                            if metadata__.is_some() {
                                return Err(serde::de::Error::duplicate_field("metadata"));
                            }
                            metadata__ = map_.next_value()?;
                        }
                        GeneratedField::MutableMetadata => {
                            if mutable_metadata__.is_some() {
                                return Err(serde::de::Error::duplicate_field("mutableMetadata"));
                            }
                            mutable_metadata__ = map_.next_value()?;
                        }
                        GeneratedField::PausedForVersion => {
                            if paused_for_version__.is_some() {
                                return Err(serde::de::Error::duplicate_field("pausedForVersion"));
                            }
                            paused_for_version__ = map_.next_value()?;
                        }
                    }
                }
                Ok(GroupSave {
                    id: id__.unwrap_or_default(),
                    created_at_ns: created_at_ns__.unwrap_or_default(),
                    membership_state: membership_state__.unwrap_or_default(),
                    installations_last_checked: installations_last_checked__.unwrap_or_default(),
                    added_by_inbox_id: added_by_inbox_id__.unwrap_or_default(),
                    welcome_id: welcome_id__,
                    rotated_at_ns: rotated_at_ns__.unwrap_or_default(),
                    conversation_type: conversation_type__.unwrap_or_default(),
                    dm_id: dm_id__,
                    last_message_ns: last_message_ns__,
                    message_disappear_from_ns: message_disappear_from_ns__,
                    message_disappear_in_ns: message_disappear_in_ns__,
                    metadata: metadata__,
                    mutable_metadata: mutable_metadata__,
                    paused_for_version: paused_for_version__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.group_backup.GroupSave", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ImmutableMetadataSave {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.creator_inbox_id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.group_backup.ImmutableMetadataSave", len)?;
        if !self.creator_inbox_id.is_empty() {
            struct_ser.serialize_field("creatorInboxId", &self.creator_inbox_id)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ImmutableMetadataSave {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "creator_inbox_id",
            "creatorInboxId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            CreatorInboxId,
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
                            "creatorInboxId" | "creator_inbox_id" => Ok(GeneratedField::CreatorInboxId),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ImmutableMetadataSave;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.group_backup.ImmutableMetadataSave")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ImmutableMetadataSave, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut creator_inbox_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::CreatorInboxId => {
                            if creator_inbox_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("creatorInboxId"));
                            }
                            creator_inbox_id__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(ImmutableMetadataSave {
                    creator_inbox_id: creator_inbox_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.group_backup.ImmutableMetadataSave", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for MutableMetadataSave {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.attributes.is_empty() {
            len += 1;
        }
        if !self.admin_list.is_empty() {
            len += 1;
        }
        if !self.super_admin_list.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.device_sync.group_backup.MutableMetadataSave", len)?;
        if !self.attributes.is_empty() {
            struct_ser.serialize_field("attributes", &self.attributes)?;
        }
        if !self.admin_list.is_empty() {
            struct_ser.serialize_field("adminList", &self.admin_list)?;
        }
        if !self.super_admin_list.is_empty() {
            struct_ser.serialize_field("superAdminList", &self.super_admin_list)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for MutableMetadataSave {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "attributes",
            "admin_list",
            "adminList",
            "super_admin_list",
            "superAdminList",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Attributes,
            AdminList,
            SuperAdminList,
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
                            "attributes" => Ok(GeneratedField::Attributes),
                            "adminList" | "admin_list" => Ok(GeneratedField::AdminList),
                            "superAdminList" | "super_admin_list" => Ok(GeneratedField::SuperAdminList),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = MutableMetadataSave;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.device_sync.group_backup.MutableMetadataSave")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<MutableMetadataSave, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut attributes__ = None;
                let mut admin_list__ = None;
                let mut super_admin_list__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Attributes => {
                            if attributes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("attributes"));
                            }
                            attributes__ = Some(
                                map_.next_value::<std::collections::HashMap<_, _>>()?
                            );
                        }
                        GeneratedField::AdminList => {
                            if admin_list__.is_some() {
                                return Err(serde::de::Error::duplicate_field("adminList"));
                            }
                            admin_list__ = Some(map_.next_value()?);
                        }
                        GeneratedField::SuperAdminList => {
                            if super_admin_list__.is_some() {
                                return Err(serde::de::Error::duplicate_field("superAdminList"));
                            }
                            super_admin_list__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(MutableMetadataSave {
                    attributes: attributes__.unwrap_or_default(),
                    admin_list: admin_list__.unwrap_or_default(),
                    super_admin_list: super_admin_list__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.device_sync.group_backup.MutableMetadataSave", FIELDS, GeneratedVisitor)
    }
}
