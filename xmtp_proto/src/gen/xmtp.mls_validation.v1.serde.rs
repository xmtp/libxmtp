impl serde::Serialize for GetAssociationStateRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.old_updates.is_empty() {
            len += 1;
        }
        if !self.new_updates.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls_validation.v1.GetAssociationStateRequest", len)?;
        if !self.old_updates.is_empty() {
            struct_ser.serialize_field("old_updates", &self.old_updates)?;
        }
        if !self.new_updates.is_empty() {
            struct_ser.serialize_field("new_updates", &self.new_updates)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GetAssociationStateRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "old_updates",
            "oldUpdates",
            "new_updates",
            "newUpdates",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            OldUpdates,
            NewUpdates,
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
                            "oldUpdates" | "old_updates" => Ok(GeneratedField::OldUpdates),
                            "newUpdates" | "new_updates" => Ok(GeneratedField::NewUpdates),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GetAssociationStateRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls_validation.v1.GetAssociationStateRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GetAssociationStateRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut old_updates__ = None;
                let mut new_updates__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::OldUpdates => {
                            if old_updates__.is_some() {
                                return Err(serde::de::Error::duplicate_field("oldUpdates"));
                            }
                            old_updates__ = Some(map_.next_value()?);
                        }
                        GeneratedField::NewUpdates => {
                            if new_updates__.is_some() {
                                return Err(serde::de::Error::duplicate_field("newUpdates"));
                            }
                            new_updates__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(GetAssociationStateRequest {
                    old_updates: old_updates__.unwrap_or_default(),
                    new_updates: new_updates__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls_validation.v1.GetAssociationStateRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GetAssociationStateResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.association_state.is_some() {
            len += 1;
        }
        if self.state_diff.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls_validation.v1.GetAssociationStateResponse", len)?;
        if let Some(v) = self.association_state.as_ref() {
            struct_ser.serialize_field("association_state", v)?;
        }
        if let Some(v) = self.state_diff.as_ref() {
            struct_ser.serialize_field("state_diff", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GetAssociationStateResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "association_state",
            "associationState",
            "state_diff",
            "stateDiff",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AssociationState,
            StateDiff,
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
                            "associationState" | "association_state" => Ok(GeneratedField::AssociationState),
                            "stateDiff" | "state_diff" => Ok(GeneratedField::StateDiff),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GetAssociationStateResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls_validation.v1.GetAssociationStateResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GetAssociationStateResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut association_state__ = None;
                let mut state_diff__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AssociationState => {
                            if association_state__.is_some() {
                                return Err(serde::de::Error::duplicate_field("associationState"));
                            }
                            association_state__ = map_.next_value()?;
                        }
                        GeneratedField::StateDiff => {
                            if state_diff__.is_some() {
                                return Err(serde::de::Error::duplicate_field("stateDiff"));
                            }
                            state_diff__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(GetAssociationStateResponse {
                    association_state: association_state__,
                    state_diff: state_diff__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls_validation.v1.GetAssociationStateResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ValidateGroupMessagesRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.group_messages.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls_validation.v1.ValidateGroupMessagesRequest", len)?;
        if !self.group_messages.is_empty() {
            struct_ser.serialize_field("group_messages", &self.group_messages)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ValidateGroupMessagesRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "group_messages",
            "groupMessages",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            GroupMessages,
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
                            "groupMessages" | "group_messages" => Ok(GeneratedField::GroupMessages),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ValidateGroupMessagesRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls_validation.v1.ValidateGroupMessagesRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ValidateGroupMessagesRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut group_messages__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::GroupMessages => {
                            if group_messages__.is_some() {
                                return Err(serde::de::Error::duplicate_field("groupMessages"));
                            }
                            group_messages__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ValidateGroupMessagesRequest {
                    group_messages: group_messages__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls_validation.v1.ValidateGroupMessagesRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for validate_group_messages_request::GroupMessage {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.group_message_bytes_tls_serialized.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls_validation.v1.ValidateGroupMessagesRequest.GroupMessage", len)?;
        if !self.group_message_bytes_tls_serialized.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("group_message_bytes_tls_serialized", pbjson::private::base64::encode(&self.group_message_bytes_tls_serialized).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for validate_group_messages_request::GroupMessage {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "group_message_bytes_tls_serialized",
            "groupMessageBytesTlsSerialized",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            GroupMessageBytesTlsSerialized,
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
                            "groupMessageBytesTlsSerialized" | "group_message_bytes_tls_serialized" => Ok(GeneratedField::GroupMessageBytesTlsSerialized),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = validate_group_messages_request::GroupMessage;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls_validation.v1.ValidateGroupMessagesRequest.GroupMessage")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<validate_group_messages_request::GroupMessage, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut group_message_bytes_tls_serialized__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::GroupMessageBytesTlsSerialized => {
                            if group_message_bytes_tls_serialized__.is_some() {
                                return Err(serde::de::Error::duplicate_field("groupMessageBytesTlsSerialized"));
                            }
                            group_message_bytes_tls_serialized__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(validate_group_messages_request::GroupMessage {
                    group_message_bytes_tls_serialized: group_message_bytes_tls_serialized__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls_validation.v1.ValidateGroupMessagesRequest.GroupMessage", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ValidateGroupMessagesResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.responses.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls_validation.v1.ValidateGroupMessagesResponse", len)?;
        if !self.responses.is_empty() {
            struct_ser.serialize_field("responses", &self.responses)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ValidateGroupMessagesResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "responses",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Responses,
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
                            "responses" => Ok(GeneratedField::Responses),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ValidateGroupMessagesResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls_validation.v1.ValidateGroupMessagesResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ValidateGroupMessagesResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut responses__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Responses => {
                            if responses__.is_some() {
                                return Err(serde::de::Error::duplicate_field("responses"));
                            }
                            responses__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ValidateGroupMessagesResponse {
                    responses: responses__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls_validation.v1.ValidateGroupMessagesResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for validate_group_messages_response::ValidationResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.is_ok {
            len += 1;
        }
        if !self.error_message.is_empty() {
            len += 1;
        }
        if !self.group_id.is_empty() {
            len += 1;
        }
        if self.is_commit {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls_validation.v1.ValidateGroupMessagesResponse.ValidationResponse", len)?;
        if self.is_ok {
            struct_ser.serialize_field("is_ok", &self.is_ok)?;
        }
        if !self.error_message.is_empty() {
            struct_ser.serialize_field("error_message", &self.error_message)?;
        }
        if !self.group_id.is_empty() {
            struct_ser.serialize_field("group_id", &self.group_id)?;
        }
        if self.is_commit {
            struct_ser.serialize_field("is_commit", &self.is_commit)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for validate_group_messages_response::ValidationResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "is_ok",
            "isOk",
            "error_message",
            "errorMessage",
            "group_id",
            "groupId",
            "is_commit",
            "isCommit",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            IsOk,
            ErrorMessage,
            GroupId,
            IsCommit,
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
                            "isOk" | "is_ok" => Ok(GeneratedField::IsOk),
                            "errorMessage" | "error_message" => Ok(GeneratedField::ErrorMessage),
                            "groupId" | "group_id" => Ok(GeneratedField::GroupId),
                            "isCommit" | "is_commit" => Ok(GeneratedField::IsCommit),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = validate_group_messages_response::ValidationResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls_validation.v1.ValidateGroupMessagesResponse.ValidationResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<validate_group_messages_response::ValidationResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut is_ok__ = None;
                let mut error_message__ = None;
                let mut group_id__ = None;
                let mut is_commit__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::IsOk => {
                            if is_ok__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isOk"));
                            }
                            is_ok__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ErrorMessage => {
                            if error_message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("errorMessage"));
                            }
                            error_message__ = Some(map_.next_value()?);
                        }
                        GeneratedField::GroupId => {
                            if group_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("groupId"));
                            }
                            group_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::IsCommit => {
                            if is_commit__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isCommit"));
                            }
                            is_commit__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(validate_group_messages_response::ValidationResponse {
                    is_ok: is_ok__.unwrap_or_default(),
                    error_message: error_message__.unwrap_or_default(),
                    group_id: group_id__.unwrap_or_default(),
                    is_commit: is_commit__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls_validation.v1.ValidateGroupMessagesResponse.ValidationResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ValidateInboxIdKeyPackagesRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.key_packages.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls_validation.v1.ValidateInboxIdKeyPackagesRequest", len)?;
        if !self.key_packages.is_empty() {
            struct_ser.serialize_field("key_packages", &self.key_packages)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ValidateInboxIdKeyPackagesRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "key_packages",
            "keyPackages",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            KeyPackages,
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
                            "keyPackages" | "key_packages" => Ok(GeneratedField::KeyPackages),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ValidateInboxIdKeyPackagesRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls_validation.v1.ValidateInboxIdKeyPackagesRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ValidateInboxIdKeyPackagesRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut key_packages__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::KeyPackages => {
                            if key_packages__.is_some() {
                                return Err(serde::de::Error::duplicate_field("keyPackages"));
                            }
                            key_packages__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ValidateInboxIdKeyPackagesRequest {
                    key_packages: key_packages__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls_validation.v1.ValidateInboxIdKeyPackagesRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for validate_inbox_id_key_packages_request::KeyPackage {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.key_package_bytes_tls_serialized.is_empty() {
            len += 1;
        }
        if self.is_inbox_id_credential {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls_validation.v1.ValidateInboxIdKeyPackagesRequest.KeyPackage", len)?;
        if !self.key_package_bytes_tls_serialized.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("key_package_bytes_tls_serialized", pbjson::private::base64::encode(&self.key_package_bytes_tls_serialized).as_str())?;
        }
        if self.is_inbox_id_credential {
            struct_ser.serialize_field("is_inbox_id_credential", &self.is_inbox_id_credential)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for validate_inbox_id_key_packages_request::KeyPackage {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "key_package_bytes_tls_serialized",
            "keyPackageBytesTlsSerialized",
            "is_inbox_id_credential",
            "isInboxIdCredential",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            KeyPackageBytesTlsSerialized,
            IsInboxIdCredential,
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
                            "keyPackageBytesTlsSerialized" | "key_package_bytes_tls_serialized" => Ok(GeneratedField::KeyPackageBytesTlsSerialized),
                            "isInboxIdCredential" | "is_inbox_id_credential" => Ok(GeneratedField::IsInboxIdCredential),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = validate_inbox_id_key_packages_request::KeyPackage;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls_validation.v1.ValidateInboxIdKeyPackagesRequest.KeyPackage")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<validate_inbox_id_key_packages_request::KeyPackage, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut key_package_bytes_tls_serialized__ = None;
                let mut is_inbox_id_credential__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::KeyPackageBytesTlsSerialized => {
                            if key_package_bytes_tls_serialized__.is_some() {
                                return Err(serde::de::Error::duplicate_field("keyPackageBytesTlsSerialized"));
                            }
                            key_package_bytes_tls_serialized__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::IsInboxIdCredential => {
                            if is_inbox_id_credential__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isInboxIdCredential"));
                            }
                            is_inbox_id_credential__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(validate_inbox_id_key_packages_request::KeyPackage {
                    key_package_bytes_tls_serialized: key_package_bytes_tls_serialized__.unwrap_or_default(),
                    is_inbox_id_credential: is_inbox_id_credential__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls_validation.v1.ValidateInboxIdKeyPackagesRequest.KeyPackage", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ValidateInboxIdKeyPackagesResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.responses.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls_validation.v1.ValidateInboxIdKeyPackagesResponse", len)?;
        if !self.responses.is_empty() {
            struct_ser.serialize_field("responses", &self.responses)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ValidateInboxIdKeyPackagesResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "responses",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Responses,
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
                            "responses" => Ok(GeneratedField::Responses),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ValidateInboxIdKeyPackagesResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls_validation.v1.ValidateInboxIdKeyPackagesResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ValidateInboxIdKeyPackagesResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut responses__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Responses => {
                            if responses__.is_some() {
                                return Err(serde::de::Error::duplicate_field("responses"));
                            }
                            responses__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ValidateInboxIdKeyPackagesResponse {
                    responses: responses__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls_validation.v1.ValidateInboxIdKeyPackagesResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for validate_inbox_id_key_packages_response::Response {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.is_ok {
            len += 1;
        }
        if !self.error_message.is_empty() {
            len += 1;
        }
        if self.credential.is_some() {
            len += 1;
        }
        if !self.installation_public_key.is_empty() {
            len += 1;
        }
        if self.expiration != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls_validation.v1.ValidateInboxIdKeyPackagesResponse.Response", len)?;
        if self.is_ok {
            struct_ser.serialize_field("is_ok", &self.is_ok)?;
        }
        if !self.error_message.is_empty() {
            struct_ser.serialize_field("error_message", &self.error_message)?;
        }
        if let Some(v) = self.credential.as_ref() {
            struct_ser.serialize_field("credential", v)?;
        }
        if !self.installation_public_key.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("installation_public_key", pbjson::private::base64::encode(&self.installation_public_key).as_str())?;
        }
        if self.expiration != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("expiration", ToString::to_string(&self.expiration).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for validate_inbox_id_key_packages_response::Response {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "is_ok",
            "isOk",
            "error_message",
            "errorMessage",
            "credential",
            "installation_public_key",
            "installationPublicKey",
            "expiration",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            IsOk,
            ErrorMessage,
            Credential,
            InstallationPublicKey,
            Expiration,
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
                            "isOk" | "is_ok" => Ok(GeneratedField::IsOk),
                            "errorMessage" | "error_message" => Ok(GeneratedField::ErrorMessage),
                            "credential" => Ok(GeneratedField::Credential),
                            "installationPublicKey" | "installation_public_key" => Ok(GeneratedField::InstallationPublicKey),
                            "expiration" => Ok(GeneratedField::Expiration),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = validate_inbox_id_key_packages_response::Response;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls_validation.v1.ValidateInboxIdKeyPackagesResponse.Response")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<validate_inbox_id_key_packages_response::Response, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut is_ok__ = None;
                let mut error_message__ = None;
                let mut credential__ = None;
                let mut installation_public_key__ = None;
                let mut expiration__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::IsOk => {
                            if is_ok__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isOk"));
                            }
                            is_ok__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ErrorMessage => {
                            if error_message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("errorMessage"));
                            }
                            error_message__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Credential => {
                            if credential__.is_some() {
                                return Err(serde::de::Error::duplicate_field("credential"));
                            }
                            credential__ = map_.next_value()?;
                        }
                        GeneratedField::InstallationPublicKey => {
                            if installation_public_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationPublicKey"));
                            }
                            installation_public_key__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Expiration => {
                            if expiration__.is_some() {
                                return Err(serde::de::Error::duplicate_field("expiration"));
                            }
                            expiration__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(validate_inbox_id_key_packages_response::Response {
                    is_ok: is_ok__.unwrap_or_default(),
                    error_message: error_message__.unwrap_or_default(),
                    credential: credential__,
                    installation_public_key: installation_public_key__.unwrap_or_default(),
                    expiration: expiration__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls_validation.v1.ValidateInboxIdKeyPackagesResponse.Response", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ValidateKeyPackagesRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.key_packages.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls_validation.v1.ValidateKeyPackagesRequest", len)?;
        if !self.key_packages.is_empty() {
            struct_ser.serialize_field("key_packages", &self.key_packages)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ValidateKeyPackagesRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "key_packages",
            "keyPackages",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            KeyPackages,
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
                            "keyPackages" | "key_packages" => Ok(GeneratedField::KeyPackages),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ValidateKeyPackagesRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls_validation.v1.ValidateKeyPackagesRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ValidateKeyPackagesRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut key_packages__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::KeyPackages => {
                            if key_packages__.is_some() {
                                return Err(serde::de::Error::duplicate_field("keyPackages"));
                            }
                            key_packages__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ValidateKeyPackagesRequest {
                    key_packages: key_packages__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls_validation.v1.ValidateKeyPackagesRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for validate_key_packages_request::KeyPackage {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.key_package_bytes_tls_serialized.is_empty() {
            len += 1;
        }
        if self.is_inbox_id_credential {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls_validation.v1.ValidateKeyPackagesRequest.KeyPackage", len)?;
        if !self.key_package_bytes_tls_serialized.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("key_package_bytes_tls_serialized", pbjson::private::base64::encode(&self.key_package_bytes_tls_serialized).as_str())?;
        }
        if self.is_inbox_id_credential {
            struct_ser.serialize_field("is_inbox_id_credential", &self.is_inbox_id_credential)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for validate_key_packages_request::KeyPackage {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "key_package_bytes_tls_serialized",
            "keyPackageBytesTlsSerialized",
            "is_inbox_id_credential",
            "isInboxIdCredential",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            KeyPackageBytesTlsSerialized,
            IsInboxIdCredential,
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
                            "keyPackageBytesTlsSerialized" | "key_package_bytes_tls_serialized" => Ok(GeneratedField::KeyPackageBytesTlsSerialized),
                            "isInboxIdCredential" | "is_inbox_id_credential" => Ok(GeneratedField::IsInboxIdCredential),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = validate_key_packages_request::KeyPackage;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls_validation.v1.ValidateKeyPackagesRequest.KeyPackage")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<validate_key_packages_request::KeyPackage, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut key_package_bytes_tls_serialized__ = None;
                let mut is_inbox_id_credential__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::KeyPackageBytesTlsSerialized => {
                            if key_package_bytes_tls_serialized__.is_some() {
                                return Err(serde::de::Error::duplicate_field("keyPackageBytesTlsSerialized"));
                            }
                            key_package_bytes_tls_serialized__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::IsInboxIdCredential => {
                            if is_inbox_id_credential__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isInboxIdCredential"));
                            }
                            is_inbox_id_credential__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(validate_key_packages_request::KeyPackage {
                    key_package_bytes_tls_serialized: key_package_bytes_tls_serialized__.unwrap_or_default(),
                    is_inbox_id_credential: is_inbox_id_credential__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls_validation.v1.ValidateKeyPackagesRequest.KeyPackage", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ValidateKeyPackagesResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.responses.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls_validation.v1.ValidateKeyPackagesResponse", len)?;
        if !self.responses.is_empty() {
            struct_ser.serialize_field("responses", &self.responses)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ValidateKeyPackagesResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "responses",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Responses,
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
                            "responses" => Ok(GeneratedField::Responses),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ValidateKeyPackagesResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls_validation.v1.ValidateKeyPackagesResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ValidateKeyPackagesResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut responses__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Responses => {
                            if responses__.is_some() {
                                return Err(serde::de::Error::duplicate_field("responses"));
                            }
                            responses__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ValidateKeyPackagesResponse {
                    responses: responses__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls_validation.v1.ValidateKeyPackagesResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for validate_key_packages_response::ValidationResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.is_ok {
            len += 1;
        }
        if !self.error_message.is_empty() {
            len += 1;
        }
        if !self.installation_id.is_empty() {
            len += 1;
        }
        if !self.account_address.is_empty() {
            len += 1;
        }
        if !self.credential_identity_bytes.is_empty() {
            len += 1;
        }
        if self.expiration != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls_validation.v1.ValidateKeyPackagesResponse.ValidationResponse", len)?;
        if self.is_ok {
            struct_ser.serialize_field("is_ok", &self.is_ok)?;
        }
        if !self.error_message.is_empty() {
            struct_ser.serialize_field("error_message", &self.error_message)?;
        }
        if !self.installation_id.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("installation_id", pbjson::private::base64::encode(&self.installation_id).as_str())?;
        }
        if !self.account_address.is_empty() {
            struct_ser.serialize_field("account_address", &self.account_address)?;
        }
        if !self.credential_identity_bytes.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("credential_identity_bytes", pbjson::private::base64::encode(&self.credential_identity_bytes).as_str())?;
        }
        if self.expiration != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("expiration", ToString::to_string(&self.expiration).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for validate_key_packages_response::ValidationResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "is_ok",
            "isOk",
            "error_message",
            "errorMessage",
            "installation_id",
            "installationId",
            "account_address",
            "accountAddress",
            "credential_identity_bytes",
            "credentialIdentityBytes",
            "expiration",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            IsOk,
            ErrorMessage,
            InstallationId,
            AccountAddress,
            CredentialIdentityBytes,
            Expiration,
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
                            "isOk" | "is_ok" => Ok(GeneratedField::IsOk),
                            "errorMessage" | "error_message" => Ok(GeneratedField::ErrorMessage),
                            "installationId" | "installation_id" => Ok(GeneratedField::InstallationId),
                            "accountAddress" | "account_address" => Ok(GeneratedField::AccountAddress),
                            "credentialIdentityBytes" | "credential_identity_bytes" => Ok(GeneratedField::CredentialIdentityBytes),
                            "expiration" => Ok(GeneratedField::Expiration),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = validate_key_packages_response::ValidationResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls_validation.v1.ValidateKeyPackagesResponse.ValidationResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<validate_key_packages_response::ValidationResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut is_ok__ = None;
                let mut error_message__ = None;
                let mut installation_id__ = None;
                let mut account_address__ = None;
                let mut credential_identity_bytes__ = None;
                let mut expiration__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::IsOk => {
                            if is_ok__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isOk"));
                            }
                            is_ok__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ErrorMessage => {
                            if error_message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("errorMessage"));
                            }
                            error_message__ = Some(map_.next_value()?);
                        }
                        GeneratedField::InstallationId => {
                            if installation_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationId"));
                            }
                            installation_id__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::AccountAddress => {
                            if account_address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("accountAddress"));
                            }
                            account_address__ = Some(map_.next_value()?);
                        }
                        GeneratedField::CredentialIdentityBytes => {
                            if credential_identity_bytes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("credentialIdentityBytes"));
                            }
                            credential_identity_bytes__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Expiration => {
                            if expiration__.is_some() {
                                return Err(serde::de::Error::duplicate_field("expiration"));
                            }
                            expiration__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(validate_key_packages_response::ValidationResponse {
                    is_ok: is_ok__.unwrap_or_default(),
                    error_message: error_message__.unwrap_or_default(),
                    installation_id: installation_id__.unwrap_or_default(),
                    account_address: account_address__.unwrap_or_default(),
                    credential_identity_bytes: credential_identity_bytes__.unwrap_or_default(),
                    expiration: expiration__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls_validation.v1.ValidateKeyPackagesResponse.ValidationResponse", FIELDS, GeneratedVisitor)
    }
}
