// @generated
impl serde::Serialize for AssociationTextVersion {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "ASSOCIATION_TEXT_VERSION_UNSPECIFIED",
            Self::AssociationTextVersion1 => "ASSOCIATION_TEXT_VERSION_1",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for AssociationTextVersion {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "ASSOCIATION_TEXT_VERSION_UNSPECIFIED",
            "ASSOCIATION_TEXT_VERSION_1",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AssociationTextVersion;

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
                    "ASSOCIATION_TEXT_VERSION_UNSPECIFIED" => Ok(AssociationTextVersion::Unspecified),
                    "ASSOCIATION_TEXT_VERSION_1" => Ok(AssociationTextVersion::AssociationTextVersion1),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
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

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ContentTypeId, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut authority_id__ = None;
                let mut type_id__ = None;
                let mut version_major__ = None;
                let mut version_minor__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AuthorityId => {
                            if authority_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("authorityId"));
                            }
                            authority_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::TypeId => {
                            if type_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("typeId"));
                            }
                            type_id__ = Some(map_.next_value()?);
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
impl serde::Serialize for ConversationType {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "CONVERSATION_TYPE_UNSPECIFIED",
            Self::Group => "CONVERSATION_TYPE_GROUP",
            Self::Dm => "CONVERSATION_TYPE_DM",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ConversationType {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "CONVERSATION_TYPE_UNSPECIFIED",
            "CONVERSATION_TYPE_GROUP",
            "CONVERSATION_TYPE_DM",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ConversationType;

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
                    "CONVERSATION_TYPE_UNSPECIFIED" => Ok(ConversationType::Unspecified),
                    "CONVERSATION_TYPE_GROUP" => Ok(ConversationType::Group),
                    "CONVERSATION_TYPE_DM" => Ok(ConversationType::Dm),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for CredentialRevocation {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.public_key.is_some() {
            len += 1;
        }
        if self.association.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.CredentialRevocation", len)?;
        if let Some(v) = self.public_key.as_ref() {
            match v {
                credential_revocation::PublicKey::InstallationKey(v) => {
                    #[allow(clippy::needless_borrow)]
                    struct_ser.serialize_field("installationKey", pbjson::private::base64::encode(&v).as_str())?;
                }
                credential_revocation::PublicKey::UnsignedLegacyCreateIdentityKey(v) => {
                    #[allow(clippy::needless_borrow)]
                    struct_ser.serialize_field("unsignedLegacyCreateIdentityKey", pbjson::private::base64::encode(&v).as_str())?;
                }
            }
        }
        if let Some(v) = self.association.as_ref() {
            match v {
                credential_revocation::Association::MessagingAccess(v) => {
                    struct_ser.serialize_field("messagingAccess", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for CredentialRevocation {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "installation_key",
            "installationKey",
            "unsigned_legacy_create_identity_key",
            "unsignedLegacyCreateIdentityKey",
            "messaging_access",
            "messagingAccess",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InstallationKey,
            UnsignedLegacyCreateIdentityKey,
            MessagingAccess,
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
                            "installationKey" | "installation_key" => Ok(GeneratedField::InstallationKey),
                            "unsignedLegacyCreateIdentityKey" | "unsigned_legacy_create_identity_key" => Ok(GeneratedField::UnsignedLegacyCreateIdentityKey),
                            "messagingAccess" | "messaging_access" => Ok(GeneratedField::MessagingAccess),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = CredentialRevocation;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.CredentialRevocation")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<CredentialRevocation, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut public_key__ = None;
                let mut association__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::InstallationKey => {
                            if public_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationKey"));
                            }
                            public_key__ = map_.next_value::<::std::option::Option<::pbjson::private::BytesDeserialize<_>>>()?.map(|x| credential_revocation::PublicKey::InstallationKey(x.0));
                        }
                        GeneratedField::UnsignedLegacyCreateIdentityKey => {
                            if public_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("unsignedLegacyCreateIdentityKey"));
                            }
                            public_key__ = map_.next_value::<::std::option::Option<::pbjson::private::BytesDeserialize<_>>>()?.map(|x| credential_revocation::PublicKey::UnsignedLegacyCreateIdentityKey(x.0));
                        }
                        GeneratedField::MessagingAccess => {
                            if association__.is_some() {
                                return Err(serde::de::Error::duplicate_field("messagingAccess"));
                            }
                            association__ = map_.next_value::<::std::option::Option<_>>()?.map(credential_revocation::Association::MessagingAccess)
;
                        }
                    }
                }
                Ok(CredentialRevocation {
                    public_key: public_key__,
                    association: association__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.CredentialRevocation", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EdDsaSignature {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.bytes.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.EdDsaSignature", len)?;
        if !self.bytes.is_empty() {
            #[allow(clippy::needless_borrow)]
            struct_ser.serialize_field("bytes", pbjson::private::base64::encode(&self.bytes).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EdDsaSignature {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "bytes",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Bytes,
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
                            "bytes" => Ok(GeneratedField::Bytes),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EdDsaSignature;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.EdDsaSignature")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EdDsaSignature, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut bytes__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Bytes => {
                            if bytes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("bytes"));
                            }
                            bytes__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(EdDsaSignature {
                    bytes: bytes__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.EdDsaSignature", FIELDS, GeneratedVisitor)
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
            let v = Compression::try_from(*v)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", *v)))?;
            struct_ser.serialize_field("compression", &v)?;
        }
        if !self.content.is_empty() {
            #[allow(clippy::needless_borrow)]
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

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EncodedContent, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut r#type__ = None;
                let mut parameters__ = None;
                let mut fallback__ = None;
                let mut compression__ = None;
                let mut content__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Type => {
                            if r#type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("type"));
                            }
                            r#type__ = map_.next_value()?;
                        }
                        GeneratedField::Parameters => {
                            if parameters__.is_some() {
                                return Err(serde::de::Error::duplicate_field("parameters"));
                            }
                            parameters__ = Some(
                                map_.next_value::<std::collections::HashMap<_, _>>()?
                            );
                        }
                        GeneratedField::Fallback => {
                            if fallback__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fallback"));
                            }
                            fallback__ = map_.next_value()?;
                        }
                        GeneratedField::Compression => {
                            if compression__.is_some() {
                                return Err(serde::de::Error::duplicate_field("compression"));
                            }
                            compression__ = map_.next_value::<::std::option::Option<Compression>>()?.map(|x| x as i32);
                        }
                        GeneratedField::Content => {
                            if content__.is_some() {
                                return Err(serde::de::Error::duplicate_field("content"));
                            }
                            content__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
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
impl serde::Serialize for GrantMessagingAccessAssociation {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.association_text_version != 0 {
            len += 1;
        }
        if self.signature.is_some() {
            len += 1;
        }
        if !self.account_address.is_empty() {
            len += 1;
        }
        if self.created_ns != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.GrantMessagingAccessAssociation", len)?;
        if self.association_text_version != 0 {
            let v = AssociationTextVersion::try_from(self.association_text_version)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.association_text_version)))?;
            struct_ser.serialize_field("associationTextVersion", &v)?;
        }
        if let Some(v) = self.signature.as_ref() {
            struct_ser.serialize_field("signature", v)?;
        }
        if !self.account_address.is_empty() {
            struct_ser.serialize_field("accountAddress", &self.account_address)?;
        }
        if self.created_ns != 0 {
            #[allow(clippy::needless_borrow)]
            struct_ser.serialize_field("createdNs", ToString::to_string(&self.created_ns).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GrantMessagingAccessAssociation {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "association_text_version",
            "associationTextVersion",
            "signature",
            "account_address",
            "accountAddress",
            "created_ns",
            "createdNs",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AssociationTextVersion,
            Signature,
            AccountAddress,
            CreatedNs,
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
                            "associationTextVersion" | "association_text_version" => Ok(GeneratedField::AssociationTextVersion),
                            "signature" => Ok(GeneratedField::Signature),
                            "accountAddress" | "account_address" => Ok(GeneratedField::AccountAddress),
                            "createdNs" | "created_ns" => Ok(GeneratedField::CreatedNs),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GrantMessagingAccessAssociation;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.GrantMessagingAccessAssociation")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GrantMessagingAccessAssociation, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut association_text_version__ = None;
                let mut signature__ = None;
                let mut account_address__ = None;
                let mut created_ns__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AssociationTextVersion => {
                            if association_text_version__.is_some() {
                                return Err(serde::de::Error::duplicate_field("associationTextVersion"));
                            }
                            association_text_version__ = Some(map_.next_value::<AssociationTextVersion>()? as i32);
                        }
                        GeneratedField::Signature => {
                            if signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("signature"));
                            }
                            signature__ = map_.next_value()?;
                        }
                        GeneratedField::AccountAddress => {
                            if account_address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("accountAddress"));
                            }
                            account_address__ = Some(map_.next_value()?);
                        }
                        GeneratedField::CreatedNs => {
                            if created_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createdNs"));
                            }
                            created_ns__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(GrantMessagingAccessAssociation {
                    association_text_version: association_text_version__.unwrap_or_default(),
                    signature: signature__,
                    account_address: account_address__.unwrap_or_default(),
                    created_ns: created_ns__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.GrantMessagingAccessAssociation", FIELDS, GeneratedVisitor)
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

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GroupMembershipChanges, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut members_added__ = None;
                let mut members_removed__ = None;
                let mut installations_added__ = None;
                let mut installations_removed__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::MembersAdded => {
                            if members_added__.is_some() {
                                return Err(serde::de::Error::duplicate_field("membersAdded"));
                            }
                            members_added__ = Some(map_.next_value()?);
                        }
                        GeneratedField::MembersRemoved => {
                            if members_removed__.is_some() {
                                return Err(serde::de::Error::duplicate_field("membersRemoved"));
                            }
                            members_removed__ = Some(map_.next_value()?);
                        }
                        GeneratedField::InstallationsAdded => {
                            if installations_added__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationsAdded"));
                            }
                            installations_added__ = Some(map_.next_value()?);
                        }
                        GeneratedField::InstallationsRemoved => {
                            if installations_removed__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationsRemoved"));
                            }
                            installations_removed__ = Some(map_.next_value()?);
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
impl serde::Serialize for GroupMetadataV1 {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.conversation_type != 0 {
            len += 1;
        }
        if !self.creator_account_address.is_empty() {
            len += 1;
        }
        if self.policies.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.GroupMetadataV1", len)?;
        if self.conversation_type != 0 {
            let v = ConversationType::try_from(self.conversation_type)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.conversation_type)))?;
            struct_ser.serialize_field("conversationType", &v)?;
        }
        if !self.creator_account_address.is_empty() {
            struct_ser.serialize_field("creatorAccountAddress", &self.creator_account_address)?;
        }
        if let Some(v) = self.policies.as_ref() {
            struct_ser.serialize_field("policies", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GroupMetadataV1 {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "conversation_type",
            "conversationType",
            "creator_account_address",
            "creatorAccountAddress",
            "policies",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ConversationType,
            CreatorAccountAddress,
            Policies,
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
                            "conversationType" | "conversation_type" => Ok(GeneratedField::ConversationType),
                            "creatorAccountAddress" | "creator_account_address" => Ok(GeneratedField::CreatorAccountAddress),
                            "policies" => Ok(GeneratedField::Policies),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GroupMetadataV1;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.GroupMetadataV1")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GroupMetadataV1, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut conversation_type__ = None;
                let mut creator_account_address__ = None;
                let mut policies__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ConversationType => {
                            if conversation_type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("conversationType"));
                            }
                            conversation_type__ = Some(map_.next_value::<ConversationType>()? as i32);
                        }
                        GeneratedField::CreatorAccountAddress => {
                            if creator_account_address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("creatorAccountAddress"));
                            }
                            creator_account_address__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Policies => {
                            if policies__.is_some() {
                                return Err(serde::de::Error::duplicate_field("policies"));
                            }
                            policies__ = map_.next_value()?;
                        }
                    }
                }
                Ok(GroupMetadataV1 {
                    conversation_type: conversation_type__.unwrap_or_default(),
                    creator_account_address: creator_account_address__.unwrap_or_default(),
                    policies: policies__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.GroupMetadataV1", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for LegacyCreateIdentityAssociation {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.signature.is_some() {
            len += 1;
        }
        if self.signed_legacy_create_identity_key.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.LegacyCreateIdentityAssociation", len)?;
        if let Some(v) = self.signature.as_ref() {
            struct_ser.serialize_field("signature", v)?;
        }
        if let Some(v) = self.signed_legacy_create_identity_key.as_ref() {
            struct_ser.serialize_field("signedLegacyCreateIdentityKey", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for LegacyCreateIdentityAssociation {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "signature",
            "signed_legacy_create_identity_key",
            "signedLegacyCreateIdentityKey",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Signature,
            SignedLegacyCreateIdentityKey,
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
                            "signature" => Ok(GeneratedField::Signature),
                            "signedLegacyCreateIdentityKey" | "signed_legacy_create_identity_key" => Ok(GeneratedField::SignedLegacyCreateIdentityKey),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = LegacyCreateIdentityAssociation;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.LegacyCreateIdentityAssociation")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<LegacyCreateIdentityAssociation, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut signature__ = None;
                let mut signed_legacy_create_identity_key__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Signature => {
                            if signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("signature"));
                            }
                            signature__ = map_.next_value()?;
                        }
                        GeneratedField::SignedLegacyCreateIdentityKey => {
                            if signed_legacy_create_identity_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("signedLegacyCreateIdentityKey"));
                            }
                            signed_legacy_create_identity_key__ = map_.next_value()?;
                        }
                    }
                }
                Ok(LegacyCreateIdentityAssociation {
                    signature: signature__,
                    signed_legacy_create_identity_key: signed_legacy_create_identity_key__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.LegacyCreateIdentityAssociation", FIELDS, GeneratedVisitor)
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

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<MembershipChange, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut installation_ids__ = None;
                let mut account_address__ = None;
                let mut initiated_by_account_address__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::InstallationIds => {
                            if installation_ids__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationIds"));
                            }
                            installation_ids__ = 
                                Some(map_.next_value::<Vec<::pbjson::private::BytesDeserialize<_>>>()?
                                    .into_iter().map(|x| x.0).collect())
                            ;
                        }
                        GeneratedField::AccountAddress => {
                            if account_address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("accountAddress"));
                            }
                            account_address__ = Some(map_.next_value()?);
                        }
                        GeneratedField::InitiatedByAccountAddress => {
                            if initiated_by_account_address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("initiatedByAccountAddress"));
                            }
                            initiated_by_account_address__ = Some(map_.next_value()?);
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
impl serde::Serialize for MembershipPolicy {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.MembershipPolicy", len)?;
        if let Some(v) = self.kind.as_ref() {
            match v {
                membership_policy::Kind::Base(v) => {
                    let v = membership_policy::BasePolicy::try_from(*v)
                        .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", *v)))?;
                    struct_ser.serialize_field("base", &v)?;
                }
                membership_policy::Kind::AndCondition(v) => {
                    struct_ser.serialize_field("andCondition", v)?;
                }
                membership_policy::Kind::AnyCondition(v) => {
                    struct_ser.serialize_field("anyCondition", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for MembershipPolicy {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "base",
            "and_condition",
            "andCondition",
            "any_condition",
            "anyCondition",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Base,
            AndCondition,
            AnyCondition,
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
                            "base" => Ok(GeneratedField::Base),
                            "andCondition" | "and_condition" => Ok(GeneratedField::AndCondition),
                            "anyCondition" | "any_condition" => Ok(GeneratedField::AnyCondition),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = MembershipPolicy;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.MembershipPolicy")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<MembershipPolicy, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut kind__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Base => {
                            if kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("base"));
                            }
                            kind__ = map_.next_value::<::std::option::Option<membership_policy::BasePolicy>>()?.map(|x| membership_policy::Kind::Base(x as i32));
                        }
                        GeneratedField::AndCondition => {
                            if kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("andCondition"));
                            }
                            kind__ = map_.next_value::<::std::option::Option<_>>()?.map(membership_policy::Kind::AndCondition)
;
                        }
                        GeneratedField::AnyCondition => {
                            if kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("anyCondition"));
                            }
                            kind__ = map_.next_value::<::std::option::Option<_>>()?.map(membership_policy::Kind::AnyCondition)
;
                        }
                    }
                }
                Ok(MembershipPolicy {
                    kind: kind__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.MembershipPolicy", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for membership_policy::AndCondition {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.policies.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.MembershipPolicy.AndCondition", len)?;
        if !self.policies.is_empty() {
            struct_ser.serialize_field("policies", &self.policies)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for membership_policy::AndCondition {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "policies",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Policies,
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
                            "policies" => Ok(GeneratedField::Policies),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = membership_policy::AndCondition;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.MembershipPolicy.AndCondition")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<membership_policy::AndCondition, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut policies__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Policies => {
                            if policies__.is_some() {
                                return Err(serde::de::Error::duplicate_field("policies"));
                            }
                            policies__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(membership_policy::AndCondition {
                    policies: policies__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.MembershipPolicy.AndCondition", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for membership_policy::AnyCondition {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.policies.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.MembershipPolicy.AnyCondition", len)?;
        if !self.policies.is_empty() {
            struct_ser.serialize_field("policies", &self.policies)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for membership_policy::AnyCondition {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "policies",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Policies,
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
                            "policies" => Ok(GeneratedField::Policies),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = membership_policy::AnyCondition;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.MembershipPolicy.AnyCondition")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<membership_policy::AnyCondition, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut policies__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Policies => {
                            if policies__.is_some() {
                                return Err(serde::de::Error::duplicate_field("policies"));
                            }
                            policies__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(membership_policy::AnyCondition {
                    policies: policies__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.MembershipPolicy.AnyCondition", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for membership_policy::BasePolicy {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "BASE_POLICY_UNSPECIFIED",
            Self::Allow => "BASE_POLICY_ALLOW",
            Self::Deny => "BASE_POLICY_DENY",
            Self::AllowIfActorCreator => "BASE_POLICY_ALLOW_IF_ACTOR_CREATOR",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for membership_policy::BasePolicy {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "BASE_POLICY_UNSPECIFIED",
            "BASE_POLICY_ALLOW",
            "BASE_POLICY_DENY",
            "BASE_POLICY_ALLOW_IF_ACTOR_CREATOR",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = membership_policy::BasePolicy;

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
                    "BASE_POLICY_UNSPECIFIED" => Ok(membership_policy::BasePolicy::Unspecified),
                    "BASE_POLICY_ALLOW" => Ok(membership_policy::BasePolicy::Allow),
                    "BASE_POLICY_DENY" => Ok(membership_policy::BasePolicy::Deny),
                    "BASE_POLICY_ALLOW_IF_ACTOR_CREATOR" => Ok(membership_policy::BasePolicy::AllowIfActorCreator),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for MlsCredential {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.installation_public_key.is_empty() {
            len += 1;
        }
        if self.association.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.MlsCredential", len)?;
        if !self.installation_public_key.is_empty() {
            #[allow(clippy::needless_borrow)]
            struct_ser.serialize_field("installationPublicKey", pbjson::private::base64::encode(&self.installation_public_key).as_str())?;
        }
        if let Some(v) = self.association.as_ref() {
            match v {
                mls_credential::Association::MessagingAccess(v) => {
                    struct_ser.serialize_field("messagingAccess", v)?;
                }
                mls_credential::Association::LegacyCreateIdentity(v) => {
                    struct_ser.serialize_field("legacyCreateIdentity", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for MlsCredential {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "installation_public_key",
            "installationPublicKey",
            "messaging_access",
            "messagingAccess",
            "legacy_create_identity",
            "legacyCreateIdentity",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InstallationPublicKey,
            MessagingAccess,
            LegacyCreateIdentity,
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
                            "installationPublicKey" | "installation_public_key" => Ok(GeneratedField::InstallationPublicKey),
                            "messagingAccess" | "messaging_access" => Ok(GeneratedField::MessagingAccess),
                            "legacyCreateIdentity" | "legacy_create_identity" => Ok(GeneratedField::LegacyCreateIdentity),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = MlsCredential;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.MlsCredential")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<MlsCredential, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut installation_public_key__ = None;
                let mut association__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::InstallationPublicKey => {
                            if installation_public_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationPublicKey"));
                            }
                            installation_public_key__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::MessagingAccess => {
                            if association__.is_some() {
                                return Err(serde::de::Error::duplicate_field("messagingAccess"));
                            }
                            association__ = map_.next_value::<::std::option::Option<_>>()?.map(mls_credential::Association::MessagingAccess)
;
                        }
                        GeneratedField::LegacyCreateIdentity => {
                            if association__.is_some() {
                                return Err(serde::de::Error::duplicate_field("legacyCreateIdentity"));
                            }
                            association__ = map_.next_value::<::std::option::Option<_>>()?.map(mls_credential::Association::LegacyCreateIdentity)
;
                        }
                    }
                }
                Ok(MlsCredential {
                    installation_public_key: installation_public_key__.unwrap_or_default(),
                    association: association__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.MlsCredential", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for PlaintextEnvelope {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.PlaintextEnvelope", len)?;
        if let Some(v) = self.content.as_ref() {
            match v {
                plaintext_envelope::Content::V1(v) => {
                    struct_ser.serialize_field("v1", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for PlaintextEnvelope {
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
            type Value = PlaintextEnvelope;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.PlaintextEnvelope")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<PlaintextEnvelope, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut content__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::V1 => {
                            if content__.is_some() {
                                return Err(serde::de::Error::duplicate_field("v1"));
                            }
                            content__ = map_.next_value::<::std::option::Option<_>>()?.map(plaintext_envelope::Content::V1)
;
                        }
                    }
                }
                Ok(PlaintextEnvelope {
                    content: content__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.PlaintextEnvelope", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for plaintext_envelope::V1 {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.content.is_empty() {
            len += 1;
        }
        if !self.idempotency_key.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.PlaintextEnvelope.V1", len)?;
        if !self.content.is_empty() {
            #[allow(clippy::needless_borrow)]
            struct_ser.serialize_field("content", pbjson::private::base64::encode(&self.content).as_str())?;
        }
        if !self.idempotency_key.is_empty() {
            struct_ser.serialize_field("idempotencyKey", &self.idempotency_key)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for plaintext_envelope::V1 {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "content",
            "idempotency_key",
            "idempotencyKey",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Content,
            IdempotencyKey,
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
                            "content" => Ok(GeneratedField::Content),
                            "idempotencyKey" | "idempotency_key" => Ok(GeneratedField::IdempotencyKey),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = plaintext_envelope::V1;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.PlaintextEnvelope.V1")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<plaintext_envelope::V1, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut content__ = None;
                let mut idempotency_key__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Content => {
                            if content__.is_some() {
                                return Err(serde::de::Error::duplicate_field("content"));
                            }
                            content__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::IdempotencyKey => {
                            if idempotency_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("idempotencyKey"));
                            }
                            idempotency_key__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(plaintext_envelope::V1 {
                    content: content__.unwrap_or_default(),
                    idempotency_key: idempotency_key__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.PlaintextEnvelope.V1", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for PolicySet {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.add_member_policy.is_some() {
            len += 1;
        }
        if self.remove_member_policy.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.PolicySet", len)?;
        if let Some(v) = self.add_member_policy.as_ref() {
            struct_ser.serialize_field("addMemberPolicy", v)?;
        }
        if let Some(v) = self.remove_member_policy.as_ref() {
            struct_ser.serialize_field("removeMemberPolicy", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for PolicySet {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "add_member_policy",
            "addMemberPolicy",
            "remove_member_policy",
            "removeMemberPolicy",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AddMemberPolicy,
            RemoveMemberPolicy,
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
                            "addMemberPolicy" | "add_member_policy" => Ok(GeneratedField::AddMemberPolicy),
                            "removeMemberPolicy" | "remove_member_policy" => Ok(GeneratedField::RemoveMemberPolicy),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = PolicySet;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.PolicySet")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<PolicySet, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut add_member_policy__ = None;
                let mut remove_member_policy__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AddMemberPolicy => {
                            if add_member_policy__.is_some() {
                                return Err(serde::de::Error::duplicate_field("addMemberPolicy"));
                            }
                            add_member_policy__ = map_.next_value()?;
                        }
                        GeneratedField::RemoveMemberPolicy => {
                            if remove_member_policy__.is_some() {
                                return Err(serde::de::Error::duplicate_field("removeMemberPolicy"));
                            }
                            remove_member_policy__ = map_.next_value()?;
                        }
                    }
                }
                Ok(PolicySet {
                    add_member_policy: add_member_policy__,
                    remove_member_policy: remove_member_policy__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.PolicySet", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RecoverableEcdsaSignature {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.bytes.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.RecoverableEcdsaSignature", len)?;
        if !self.bytes.is_empty() {
            #[allow(clippy::needless_borrow)]
            struct_ser.serialize_field("bytes", pbjson::private::base64::encode(&self.bytes).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RecoverableEcdsaSignature {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "bytes",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Bytes,
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
                            "bytes" => Ok(GeneratedField::Bytes),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = RecoverableEcdsaSignature;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.RecoverableEcdsaSignature")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<RecoverableEcdsaSignature, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut bytes__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Bytes => {
                            if bytes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("bytes"));
                            }
                            bytes__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(RecoverableEcdsaSignature {
                    bytes: bytes__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.RecoverableEcdsaSignature", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RevokeMessagingAccessAssociation {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.association_text_version != 0 {
            len += 1;
        }
        if self.signature.is_some() {
            len += 1;
        }
        if !self.account_address.is_empty() {
            len += 1;
        }
        if self.created_ns != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.RevokeMessagingAccessAssociation", len)?;
        if self.association_text_version != 0 {
            let v = AssociationTextVersion::try_from(self.association_text_version)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.association_text_version)))?;
            struct_ser.serialize_field("associationTextVersion", &v)?;
        }
        if let Some(v) = self.signature.as_ref() {
            struct_ser.serialize_field("signature", v)?;
        }
        if !self.account_address.is_empty() {
            struct_ser.serialize_field("accountAddress", &self.account_address)?;
        }
        if self.created_ns != 0 {
            #[allow(clippy::needless_borrow)]
            struct_ser.serialize_field("createdNs", ToString::to_string(&self.created_ns).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RevokeMessagingAccessAssociation {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "association_text_version",
            "associationTextVersion",
            "signature",
            "account_address",
            "accountAddress",
            "created_ns",
            "createdNs",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AssociationTextVersion,
            Signature,
            AccountAddress,
            CreatedNs,
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
                            "associationTextVersion" | "association_text_version" => Ok(GeneratedField::AssociationTextVersion),
                            "signature" => Ok(GeneratedField::Signature),
                            "accountAddress" | "account_address" => Ok(GeneratedField::AccountAddress),
                            "createdNs" | "created_ns" => Ok(GeneratedField::CreatedNs),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = RevokeMessagingAccessAssociation;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.RevokeMessagingAccessAssociation")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<RevokeMessagingAccessAssociation, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut association_text_version__ = None;
                let mut signature__ = None;
                let mut account_address__ = None;
                let mut created_ns__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AssociationTextVersion => {
                            if association_text_version__.is_some() {
                                return Err(serde::de::Error::duplicate_field("associationTextVersion"));
                            }
                            association_text_version__ = Some(map_.next_value::<AssociationTextVersion>()? as i32);
                        }
                        GeneratedField::Signature => {
                            if signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("signature"));
                            }
                            signature__ = map_.next_value()?;
                        }
                        GeneratedField::AccountAddress => {
                            if account_address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("accountAddress"));
                            }
                            account_address__ = Some(map_.next_value()?);
                        }
                        GeneratedField::CreatedNs => {
                            if created_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createdNs"));
                            }
                            created_ns__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(RevokeMessagingAccessAssociation {
                    association_text_version: association_text_version__.unwrap_or_default(),
                    signature: signature__,
                    account_address: account_address__.unwrap_or_default(),
                    created_ns: created_ns__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.RevokeMessagingAccessAssociation", FIELDS, GeneratedVisitor)
    }
}
