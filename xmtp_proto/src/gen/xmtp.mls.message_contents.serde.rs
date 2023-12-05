// @generated
impl serde::Serialize for Compression {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Deflate => "COMPRESSION_DEFLATE",
            Self::Gzip => "COMPRESSION_GZIP",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for Compression {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "COMPRESSION_DEFLATE",
            "COMPRESSION_GZIP",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Compression;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(Compression::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                use std::convert::TryFrom;
                i32::try_from(v)
                    .ok()
                    .and_then(Compression::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "COMPRESSION_DEFLATE" => Ok(Compression::Deflate),
                    "COMPRESSION_GZIP" => Ok(Compression::Gzip),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ContentTypeId {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.authority_id.is_empty() {
            len += 1;
        }
        if !self.type_id.is_empty() {
            len += 1;
        }
        if self.version_major != 0 {
            len += 1;
        }
        if self.version_minor != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.ContentTypeId", len)?;
        if !self.authority_id.is_empty() {
            struct_ser.serialize_field("authorityId", &self.authority_id)?;
        }
        if !self.type_id.is_empty() {
            struct_ser.serialize_field("typeId", &self.type_id)?;
        }
        if self.version_major != 0 {
            struct_ser.serialize_field("versionMajor", &self.version_major)?;
        }
        if self.version_minor != 0 {
            struct_ser.serialize_field("versionMinor", &self.version_minor)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ContentTypeId {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "authority_id",
            "authorityId",
            "type_id",
            "typeId",
            "version_major",
            "versionMajor",
            "version_minor",
            "versionMinor",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AuthorityId,
            TypeId,
            VersionMajor,
            VersionMinor,
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
                            "authorityId" | "authority_id" => Ok(GeneratedField::AuthorityId),
                            "typeId" | "type_id" => Ok(GeneratedField::TypeId),
                            "versionMajor" | "version_major" => Ok(GeneratedField::VersionMajor),
                            "versionMinor" | "version_minor" => Ok(GeneratedField::VersionMinor),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ContentTypeId;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.ContentTypeId")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<ContentTypeId, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut authority_id__ = None;
                let mut type_id__ = None;
                let mut version_major__ = None;
                let mut version_minor__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::AuthorityId => {
                            if authority_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("authorityId"));
                            }
                            authority_id__ = Some(map.next_value()?);
                        }
                        GeneratedField::TypeId => {
                            if type_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("typeId"));
                            }
                            type_id__ = Some(map.next_value()?);
                        }
                        GeneratedField::VersionMajor => {
                            if version_major__.is_some() {
                                return Err(serde::de::Error::duplicate_field("versionMajor"));
                            }
                            version_major__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::VersionMinor => {
                            if version_minor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("versionMinor"));
                            }
                            version_minor__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(ContentTypeId {
                    authority_id: authority_id__.unwrap_or_default(),
                    type_id: type_id__.unwrap_or_default(),
                    version_major: version_major__.unwrap_or_default(),
                    version_minor: version_minor__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.ContentTypeId", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EncodedContent {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.r#type.is_some() {
            len += 1;
        }
        if !self.parameters.is_empty() {
            len += 1;
        }
        if self.fallback.is_some() {
            len += 1;
        }
        if self.compression.is_some() {
            len += 1;
        }
        if !self.content.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.EncodedContent", len)?;
        if let Some(v) = self.r#type.as_ref() {
            struct_ser.serialize_field("type", v)?;
        }
        if !self.parameters.is_empty() {
            struct_ser.serialize_field("parameters", &self.parameters)?;
        }
        if let Some(v) = self.fallback.as_ref() {
            struct_ser.serialize_field("fallback", v)?;
        }
        if let Some(v) = self.compression.as_ref() {
            let v = Compression::from_i32(*v)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", *v)))?;
            struct_ser.serialize_field("compression", &v)?;
        }
        if !self.content.is_empty() {
            struct_ser.serialize_field("content", pbjson::private::base64::encode(&self.content).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EncodedContent {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "type",
            "parameters",
            "fallback",
            "compression",
            "content",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Type,
            Parameters,
            Fallback,
            Compression,
            Content,
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
                            "type" => Ok(GeneratedField::Type),
                            "parameters" => Ok(GeneratedField::Parameters),
                            "fallback" => Ok(GeneratedField::Fallback),
                            "compression" => Ok(GeneratedField::Compression),
                            "content" => Ok(GeneratedField::Content),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EncodedContent;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.EncodedContent")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<EncodedContent, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut r#type__ = None;
                let mut parameters__ = None;
                let mut fallback__ = None;
                let mut compression__ = None;
                let mut content__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Type => {
                            if r#type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("type"));
                            }
                            r#type__ = map.next_value()?;
                        }
                        GeneratedField::Parameters => {
                            if parameters__.is_some() {
                                return Err(serde::de::Error::duplicate_field("parameters"));
                            }
                            parameters__ = Some(
                                map.next_value::<std::collections::HashMap<_, _>>()?
                            );
                        }
                        GeneratedField::Fallback => {
                            if fallback__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fallback"));
                            }
                            fallback__ = map.next_value()?;
                        }
                        GeneratedField::Compression => {
                            if compression__.is_some() {
                                return Err(serde::de::Error::duplicate_field("compression"));
                            }
                            compression__ = map.next_value::<::std::option::Option<Compression>>()?.map(|x| x as i32);
                        }
                        GeneratedField::Content => {
                            if content__.is_some() {
                                return Err(serde::de::Error::duplicate_field("content"));
                            }
                            content__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(EncodedContent {
                    r#type: r#type__,
                    parameters: parameters__.unwrap_or_default(),
                    fallback: fallback__,
                    compression: compression__,
                    content: content__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.EncodedContent", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GroupMembershipChanges {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.members_added.is_empty() {
            len += 1;
        }
        if !self.members_removed.is_empty() {
            len += 1;
        }
        if !self.installations_added.is_empty() {
            len += 1;
        }
        if !self.installations_removed.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.GroupMembershipChanges", len)?;
        if !self.members_added.is_empty() {
            struct_ser.serialize_field("membersAdded", &self.members_added)?;
        }
        if !self.members_removed.is_empty() {
            struct_ser.serialize_field("membersRemoved", &self.members_removed)?;
        }
        if !self.installations_added.is_empty() {
            struct_ser.serialize_field("installationsAdded", &self.installations_added)?;
        }
        if !self.installations_removed.is_empty() {
            struct_ser.serialize_field("installationsRemoved", &self.installations_removed)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GroupMembershipChanges {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "members_added",
            "membersAdded",
            "members_removed",
            "membersRemoved",
            "installations_added",
            "installationsAdded",
            "installations_removed",
            "installationsRemoved",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            MembersAdded,
            MembersRemoved,
            InstallationsAdded,
            InstallationsRemoved,
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
                            "membersAdded" | "members_added" => Ok(GeneratedField::MembersAdded),
                            "membersRemoved" | "members_removed" => Ok(GeneratedField::MembersRemoved),
                            "installationsAdded" | "installations_added" => Ok(GeneratedField::InstallationsAdded),
                            "installationsRemoved" | "installations_removed" => Ok(GeneratedField::InstallationsRemoved),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GroupMembershipChanges;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.GroupMembershipChanges")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<GroupMembershipChanges, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut members_added__ = None;
                let mut members_removed__ = None;
                let mut installations_added__ = None;
                let mut installations_removed__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::MembersAdded => {
                            if members_added__.is_some() {
                                return Err(serde::de::Error::duplicate_field("membersAdded"));
                            }
                            members_added__ = Some(map.next_value()?);
                        }
                        GeneratedField::MembersRemoved => {
                            if members_removed__.is_some() {
                                return Err(serde::de::Error::duplicate_field("membersRemoved"));
                            }
                            members_removed__ = Some(map.next_value()?);
                        }
                        GeneratedField::InstallationsAdded => {
                            if installations_added__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationsAdded"));
                            }
                            installations_added__ = Some(map.next_value()?);
                        }
                        GeneratedField::InstallationsRemoved => {
                            if installations_removed__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationsRemoved"));
                            }
                            installations_removed__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(GroupMembershipChanges {
                    members_added: members_added__.unwrap_or_default(),
                    members_removed: members_removed__.unwrap_or_default(),
                    installations_added: installations_added__.unwrap_or_default(),
                    installations_removed: installations_removed__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.GroupMembershipChanges", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GroupMessage {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.version.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.GroupMessage", len)?;
        if let Some(v) = self.version.as_ref() {
            match v {
                group_message::Version::V1(v) => {
                    struct_ser.serialize_field("v1", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GroupMessage {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "v1",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            V1,
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
                            "v1" => Ok(GeneratedField::V1),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GroupMessage;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.GroupMessage")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<GroupMessage, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut version__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::V1 => {
                            if version__.is_some() {
                                return Err(serde::de::Error::duplicate_field("v1"));
                            }
                            version__ = map.next_value::<::std::option::Option<_>>()?.map(group_message::Version::V1)
;
                        }
                    }
                }
                Ok(GroupMessage {
                    version: version__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.GroupMessage", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for group_message::V1 {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.mls_message_tls_serialized.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.GroupMessage.V1", len)?;
        if !self.mls_message_tls_serialized.is_empty() {
            struct_ser.serialize_field("mlsMessageTlsSerialized", pbjson::private::base64::encode(&self.mls_message_tls_serialized).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for group_message::V1 {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "mls_message_tls_serialized",
            "mlsMessageTlsSerialized",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            MlsMessageTlsSerialized,
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
                            "mlsMessageTlsSerialized" | "mls_message_tls_serialized" => Ok(GeneratedField::MlsMessageTlsSerialized),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = group_message::V1;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.GroupMessage.V1")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<group_message::V1, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut mls_message_tls_serialized__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::MlsMessageTlsSerialized => {
                            if mls_message_tls_serialized__.is_some() {
                                return Err(serde::de::Error::duplicate_field("mlsMessageTlsSerialized"));
                            }
                            mls_message_tls_serialized__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(group_message::V1 {
                    mls_message_tls_serialized: mls_message_tls_serialized__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.GroupMessage.V1", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for MembershipChange {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.installation_ids.is_empty() {
            len += 1;
        }
        if !self.account_address.is_empty() {
            len += 1;
        }
        if !self.initiated_by_account_address.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.MembershipChange", len)?;
        if !self.installation_ids.is_empty() {
            struct_ser.serialize_field("installationIds", &self.installation_ids.iter().map(pbjson::private::base64::encode).collect::<Vec<_>>())?;
        }
        if !self.account_address.is_empty() {
            struct_ser.serialize_field("accountAddress", &self.account_address)?;
        }
        if !self.initiated_by_account_address.is_empty() {
            struct_ser.serialize_field("initiatedByAccountAddress", &self.initiated_by_account_address)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for MembershipChange {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "installation_ids",
            "installationIds",
            "account_address",
            "accountAddress",
            "initiated_by_account_address",
            "initiatedByAccountAddress",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InstallationIds,
            AccountAddress,
            InitiatedByAccountAddress,
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
                            "installationIds" | "installation_ids" => Ok(GeneratedField::InstallationIds),
                            "accountAddress" | "account_address" => Ok(GeneratedField::AccountAddress),
                            "initiatedByAccountAddress" | "initiated_by_account_address" => Ok(GeneratedField::InitiatedByAccountAddress),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = MembershipChange;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.MembershipChange")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<MembershipChange, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut installation_ids__ = None;
                let mut account_address__ = None;
                let mut initiated_by_account_address__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::InstallationIds => {
                            if installation_ids__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationIds"));
                            }
                            installation_ids__ = 
                                Some(map.next_value::<Vec<::pbjson::private::BytesDeserialize<_>>>()?
                                    .into_iter().map(|x| x.0).collect())
                            ;
                        }
                        GeneratedField::AccountAddress => {
                            if account_address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("accountAddress"));
                            }
                            account_address__ = Some(map.next_value()?);
                        }
                        GeneratedField::InitiatedByAccountAddress => {
                            if initiated_by_account_address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("initiatedByAccountAddress"));
                            }
                            initiated_by_account_address__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(MembershipChange {
                    installation_ids: installation_ids__.unwrap_or_default(),
                    account_address: account_address__.unwrap_or_default(),
                    initiated_by_account_address: initiated_by_account_address__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.MembershipChange", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for WelcomeMessage {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.version.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.WelcomeMessage", len)?;
        if let Some(v) = self.version.as_ref() {
            match v {
                welcome_message::Version::V1(v) => {
                    struct_ser.serialize_field("v1", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for WelcomeMessage {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "v1",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            V1,
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
                            "v1" => Ok(GeneratedField::V1),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = WelcomeMessage;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.WelcomeMessage")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<WelcomeMessage, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut version__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::V1 => {
                            if version__.is_some() {
                                return Err(serde::de::Error::duplicate_field("v1"));
                            }
                            version__ = map.next_value::<::std::option::Option<_>>()?.map(welcome_message::Version::V1)
;
                        }
                    }
                }
                Ok(WelcomeMessage {
                    version: version__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.WelcomeMessage", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for welcome_message::V1 {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.welcome_message_tls_serialized.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.WelcomeMessage.V1", len)?;
        if !self.welcome_message_tls_serialized.is_empty() {
            struct_ser.serialize_field("welcomeMessageTlsSerialized", pbjson::private::base64::encode(&self.welcome_message_tls_serialized).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for welcome_message::V1 {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "welcome_message_tls_serialized",
            "welcomeMessageTlsSerialized",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            WelcomeMessageTlsSerialized,
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
                            "welcomeMessageTlsSerialized" | "welcome_message_tls_serialized" => Ok(GeneratedField::WelcomeMessageTlsSerialized),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = welcome_message::V1;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.WelcomeMessage.V1")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<welcome_message::V1, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut welcome_message_tls_serialized__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::WelcomeMessageTlsSerialized => {
                            if welcome_message_tls_serialized__.is_some() {
                                return Err(serde::de::Error::duplicate_field("welcomeMessageTlsSerialized"));
                            }
                            welcome_message_tls_serialized__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(welcome_message::V1 {
                    welcome_message_tls_serialized: welcome_message_tls_serialized__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.WelcomeMessage.V1", FIELDS, GeneratedVisitor)
    }
}
