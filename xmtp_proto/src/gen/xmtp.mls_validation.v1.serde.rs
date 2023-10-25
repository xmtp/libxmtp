// @generated
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
            struct_ser.serialize_field("groupMessages", &self.group_messages)?;
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
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
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

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ValidateGroupMessagesRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut group_messages__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::GroupMessages => {
                            if group_messages__.is_some() {
                                return Err(serde::de::Error::duplicate_field("groupMessages"));
                            }
                            group_messages__ = Some(map.next_value()?);
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
            struct_ser.serialize_field("groupMessageBytesTlsSerialized", pbjson::private::base64::encode(&self.group_message_bytes_tls_serialized).as_str())?;
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
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
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

            fn visit_map<V>(self, mut map: V) -> std::result::Result<validate_group_messages_request::GroupMessage, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut group_message_bytes_tls_serialized__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::GroupMessageBytesTlsSerialized => {
                            if group_message_bytes_tls_serialized__.is_some() {
                                return Err(serde::de::Error::duplicate_field("groupMessageBytesTlsSerialized"));
                            }
                            group_message_bytes_tls_serialized__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
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
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
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

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ValidateGroupMessagesResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut responses__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Responses => {
                            if responses__.is_some() {
                                return Err(serde::de::Error::duplicate_field("responses"));
                            }
                            responses__ = Some(map.next_value()?);
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
        if self.epoch != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls_validation.v1.ValidateGroupMessagesResponse.ValidationResponse", len)?;
        if self.is_ok {
            struct_ser.serialize_field("isOk", &self.is_ok)?;
        }
        if !self.error_message.is_empty() {
            struct_ser.serialize_field("errorMessage", &self.error_message)?;
        }
        if !self.group_id.is_empty() {
            struct_ser.serialize_field("groupId", &self.group_id)?;
        }
        if self.epoch != 0 {
            struct_ser.serialize_field("epoch", ToString::to_string(&self.epoch).as_str())?;
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
            "epoch",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            IsOk,
            ErrorMessage,
            GroupId,
            Epoch,
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
                            "epoch" => Ok(GeneratedField::Epoch),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
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

            fn visit_map<V>(self, mut map: V) -> std::result::Result<validate_group_messages_response::ValidationResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut is_ok__ = None;
                let mut error_message__ = None;
                let mut group_id__ = None;
                let mut epoch__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::IsOk => {
                            if is_ok__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isOk"));
                            }
                            is_ok__ = Some(map.next_value()?);
                        }
                        GeneratedField::ErrorMessage => {
                            if error_message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("errorMessage"));
                            }
                            error_message__ = Some(map.next_value()?);
                        }
                        GeneratedField::GroupId => {
                            if group_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("groupId"));
                            }
                            group_id__ = Some(map.next_value()?);
                        }
                        GeneratedField::Epoch => {
                            if epoch__.is_some() {
                                return Err(serde::de::Error::duplicate_field("epoch"));
                            }
                            epoch__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(validate_group_messages_response::ValidationResponse {
                    is_ok: is_ok__.unwrap_or_default(),
                    error_message: error_message__.unwrap_or_default(),
                    group_id: group_id__.unwrap_or_default(),
                    epoch: epoch__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls_validation.v1.ValidateGroupMessagesResponse.ValidationResponse", FIELDS, GeneratedVisitor)
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
            struct_ser.serialize_field("keyPackages", &self.key_packages)?;
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
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
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

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ValidateKeyPackagesRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut key_packages__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::KeyPackages => {
                            if key_packages__.is_some() {
                                return Err(serde::de::Error::duplicate_field("keyPackages"));
                            }
                            key_packages__ = Some(map.next_value()?);
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls_validation.v1.ValidateKeyPackagesRequest.KeyPackage", len)?;
        if !self.key_package_bytes_tls_serialized.is_empty() {
            struct_ser.serialize_field("keyPackageBytesTlsSerialized", pbjson::private::base64::encode(&self.key_package_bytes_tls_serialized).as_str())?;
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
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            KeyPackageBytesTlsSerialized,
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
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
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

            fn visit_map<V>(self, mut map: V) -> std::result::Result<validate_key_packages_request::KeyPackage, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut key_package_bytes_tls_serialized__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::KeyPackageBytesTlsSerialized => {
                            if key_package_bytes_tls_serialized__.is_some() {
                                return Err(serde::de::Error::duplicate_field("keyPackageBytesTlsSerialized"));
                            }
                            key_package_bytes_tls_serialized__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(validate_key_packages_request::KeyPackage {
                    key_package_bytes_tls_serialized: key_package_bytes_tls_serialized__.unwrap_or_default(),
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
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
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

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ValidateKeyPackagesResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut responses__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Responses => {
                            if responses__.is_some() {
                                return Err(serde::de::Error::duplicate_field("responses"));
                            }
                            responses__ = Some(map.next_value()?);
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
        if !self.wallet_address.is_empty() {
            len += 1;
        }
        if !self.pub_key_bytes.is_empty() {
            len += 1;
        }
        if !self.credential_identity_bytes.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls_validation.v1.ValidateKeyPackagesResponse.ValidationResponse", len)?;
        if self.is_ok {
            struct_ser.serialize_field("isOk", &self.is_ok)?;
        }
        if !self.error_message.is_empty() {
            struct_ser.serialize_field("errorMessage", &self.error_message)?;
        }
        if !self.installation_id.is_empty() {
            struct_ser.serialize_field("installationId", &self.installation_id)?;
        }
        if !self.wallet_address.is_empty() {
            struct_ser.serialize_field("walletAddress", &self.wallet_address)?;
        }
        if !self.pub_key_bytes.is_empty() {
            struct_ser.serialize_field("pubKeyBytes", pbjson::private::base64::encode(&self.pub_key_bytes).as_str())?;
        }
        if !self.credential_identity_bytes.is_empty() {
            struct_ser.serialize_field("credentialIdentityBytes", pbjson::private::base64::encode(&self.credential_identity_bytes).as_str())?;
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
            "wallet_address",
            "walletAddress",
            "pub_key_bytes",
            "pubKeyBytes",
            "credential_identity_bytes",
            "credentialIdentityBytes",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            IsOk,
            ErrorMessage,
            InstallationId,
            WalletAddress,
            PubKeyBytes,
            CredentialIdentityBytes,
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
                            "walletAddress" | "wallet_address" => Ok(GeneratedField::WalletAddress),
                            "pubKeyBytes" | "pub_key_bytes" => Ok(GeneratedField::PubKeyBytes),
                            "credentialIdentityBytes" | "credential_identity_bytes" => Ok(GeneratedField::CredentialIdentityBytes),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
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

            fn visit_map<V>(self, mut map: V) -> std::result::Result<validate_key_packages_response::ValidationResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut is_ok__ = None;
                let mut error_message__ = None;
                let mut installation_id__ = None;
                let mut wallet_address__ = None;
                let mut pub_key_bytes__ = None;
                let mut credential_identity_bytes__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::IsOk => {
                            if is_ok__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isOk"));
                            }
                            is_ok__ = Some(map.next_value()?);
                        }
                        GeneratedField::ErrorMessage => {
                            if error_message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("errorMessage"));
                            }
                            error_message__ = Some(map.next_value()?);
                        }
                        GeneratedField::InstallationId => {
                            if installation_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationId"));
                            }
                            installation_id__ = Some(map.next_value()?);
                        }
                        GeneratedField::WalletAddress => {
                            if wallet_address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("walletAddress"));
                            }
                            wallet_address__ = Some(map.next_value()?);
                        }
                        GeneratedField::PubKeyBytes => {
                            if pub_key_bytes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("pubKeyBytes"));
                            }
                            pub_key_bytes__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::CredentialIdentityBytes => {
                            if credential_identity_bytes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("credentialIdentityBytes"));
                            }
                            credential_identity_bytes__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(validate_key_packages_response::ValidationResponse {
                    is_ok: is_ok__.unwrap_or_default(),
                    error_message: error_message__.unwrap_or_default(),
                    installation_id: installation_id__.unwrap_or_default(),
                    wallet_address: wallet_address__.unwrap_or_default(),
                    pub_key_bytes: pub_key_bytes__.unwrap_or_default(),
                    credential_identity_bytes: credential_identity_bytes__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls_validation.v1.ValidateKeyPackagesResponse.ValidationResponse", FIELDS, GeneratedVisitor)
    }
}
