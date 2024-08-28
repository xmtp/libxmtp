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
            Self::Sync => "CONVERSATION_TYPE_SYNC",
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
            "CONVERSATION_TYPE_SYNC",
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
                    "CONVERSATION_TYPE_SYNC" => Ok(ConversationType::Sync),
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
impl serde::Serialize for DmMembers {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.dm_member_one.is_some() {
            len += 1;
        }
        if self.dm_member_two.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.DmMembers", len)?;
        if let Some(v) = self.dm_member_one.as_ref() {
            struct_ser.serialize_field("dmMemberOne", v)?;
        }
        if let Some(v) = self.dm_member_two.as_ref() {
            struct_ser.serialize_field("dmMemberTwo", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for DmMembers {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "dm_member_one",
            "dmMemberOne",
            "dm_member_two",
            "dmMemberTwo",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            DmMemberOne,
            DmMemberTwo,
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
                            "dmMemberOne" | "dm_member_one" => Ok(GeneratedField::DmMemberOne),
                            "dmMemberTwo" | "dm_member_two" => Ok(GeneratedField::DmMemberTwo),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = DmMembers;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.DmMembers")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<DmMembers, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut dm_member_one__ = None;
                let mut dm_member_two__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::DmMemberOne => {
                            if dm_member_one__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dmMemberOne"));
                            }
                            dm_member_one__ = map_.next_value()?;
                        }
                        GeneratedField::DmMemberTwo => {
                            if dm_member_two__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dmMemberTwo"));
                            }
                            dm_member_two__ = map_.next_value()?;
                        }
                    }
                }
                Ok(DmMembers {
                    dm_member_one: dm_member_one__,
                    dm_member_two: dm_member_two__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.DmMembers", FIELDS, GeneratedVisitor)
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
impl serde::Serialize for GroupMembership {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.members.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.GroupMembership", len)?;
        if !self.members.is_empty() {
            let v: std::collections::HashMap<_, _> = self.members.iter()
                .map(|(k, v)| (k, v.to_string())).collect();
            struct_ser.serialize_field("members", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GroupMembership {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "members",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Members,
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
                            "members" => Ok(GeneratedField::Members),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GroupMembership;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.GroupMembership")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GroupMembership, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut members__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Members => {
                            if members__.is_some() {
                                return Err(serde::de::Error::duplicate_field("members"));
                            }
                            members__ = Some(
                                map_.next_value::<std::collections::HashMap<_, ::pbjson::private::NumberDeserialize<u64>>>()?
                                    .into_iter().map(|(k,v)| (k, v.0)).collect()
                            );
                        }
                    }
                }
                Ok(GroupMembership {
                    members: members__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.GroupMembership", FIELDS, GeneratedVisitor)
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
        if !self.creator_inbox_id.is_empty() {
            len += 1;
        }
        if self.dm_members.is_some() {
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
        if !self.creator_inbox_id.is_empty() {
            struct_ser.serialize_field("creatorInboxId", &self.creator_inbox_id)?;
        }
        if let Some(v) = self.dm_members.as_ref() {
            struct_ser.serialize_field("dmMembers", v)?;
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
            "creator_inbox_id",
            "creatorInboxId",
            "dm_members",
            "dmMembers",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ConversationType,
            CreatorAccountAddress,
            CreatorInboxId,
            DmMembers,
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
                            "creatorInboxId" | "creator_inbox_id" => Ok(GeneratedField::CreatorInboxId),
                            "dmMembers" | "dm_members" => Ok(GeneratedField::DmMembers),
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
                let mut creator_inbox_id__ = None;
                let mut dm_members__ = None;
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
                        GeneratedField::CreatorInboxId => {
                            if creator_inbox_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("creatorInboxId"));
                            }
                            creator_inbox_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::DmMembers => {
                            if dm_members__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dmMembers"));
                            }
                            dm_members__ = map_.next_value()?;
                        }
                    }
                }
                Ok(GroupMetadataV1 {
                    conversation_type: conversation_type__.unwrap_or_default(),
                    creator_account_address: creator_account_address__.unwrap_or_default(),
                    creator_inbox_id: creator_inbox_id__.unwrap_or_default(),
                    dm_members: dm_members__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.GroupMetadataV1", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GroupMutableMetadataV1 {
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
        if self.admin_list.is_some() {
            len += 1;
        }
        if self.super_admin_list.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.GroupMutableMetadataV1", len)?;
        if !self.attributes.is_empty() {
            struct_ser.serialize_field("attributes", &self.attributes)?;
        }
        if let Some(v) = self.admin_list.as_ref() {
            struct_ser.serialize_field("adminList", v)?;
        }
        if let Some(v) = self.super_admin_list.as_ref() {
            struct_ser.serialize_field("superAdminList", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GroupMutableMetadataV1 {
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
            type Value = GroupMutableMetadataV1;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.GroupMutableMetadataV1")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GroupMutableMetadataV1, V::Error>
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
                            admin_list__ = map_.next_value()?;
                        }
                        GeneratedField::SuperAdminList => {
                            if super_admin_list__.is_some() {
                                return Err(serde::de::Error::duplicate_field("superAdminList"));
                            }
                            super_admin_list__ = map_.next_value()?;
                        }
                    }
                }
                Ok(GroupMutableMetadataV1 {
                    attributes: attributes__.unwrap_or_default(),
                    admin_list: admin_list__,
                    super_admin_list: super_admin_list__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.GroupMutableMetadataV1", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GroupMutablePermissionsV1 {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.policies.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.GroupMutablePermissionsV1", len)?;
        if let Some(v) = self.policies.as_ref() {
            struct_ser.serialize_field("policies", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GroupMutablePermissionsV1 {
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
            type Value = GroupMutablePermissionsV1;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.GroupMutablePermissionsV1")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GroupMutablePermissionsV1, V::Error>
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
                            policies__ = map_.next_value()?;
                        }
                    }
                }
                Ok(GroupMutablePermissionsV1 {
                    policies: policies__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.GroupMutablePermissionsV1", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GroupUpdated {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.initiated_by_inbox_id.is_empty() {
            len += 1;
        }
        if !self.added_inboxes.is_empty() {
            len += 1;
        }
        if !self.removed_inboxes.is_empty() {
            len += 1;
        }
        if !self.metadata_field_changes.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.GroupUpdated", len)?;
        if !self.initiated_by_inbox_id.is_empty() {
            struct_ser.serialize_field("initiatedByInboxId", &self.initiated_by_inbox_id)?;
        }
        if !self.added_inboxes.is_empty() {
            struct_ser.serialize_field("addedInboxes", &self.added_inboxes)?;
        }
        if !self.removed_inboxes.is_empty() {
            struct_ser.serialize_field("removedInboxes", &self.removed_inboxes)?;
        }
        if !self.metadata_field_changes.is_empty() {
            struct_ser.serialize_field("metadataFieldChanges", &self.metadata_field_changes)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GroupUpdated {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "initiated_by_inbox_id",
            "initiatedByInboxId",
            "added_inboxes",
            "addedInboxes",
            "removed_inboxes",
            "removedInboxes",
            "metadata_field_changes",
            "metadataFieldChanges",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InitiatedByInboxId,
            AddedInboxes,
            RemovedInboxes,
            MetadataFieldChanges,
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
                            "initiatedByInboxId" | "initiated_by_inbox_id" => Ok(GeneratedField::InitiatedByInboxId),
                            "addedInboxes" | "added_inboxes" => Ok(GeneratedField::AddedInboxes),
                            "removedInboxes" | "removed_inboxes" => Ok(GeneratedField::RemovedInboxes),
                            "metadataFieldChanges" | "metadata_field_changes" => Ok(GeneratedField::MetadataFieldChanges),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GroupUpdated;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.GroupUpdated")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GroupUpdated, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut initiated_by_inbox_id__ = None;
                let mut added_inboxes__ = None;
                let mut removed_inboxes__ = None;
                let mut metadata_field_changes__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::InitiatedByInboxId => {
                            if initiated_by_inbox_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("initiatedByInboxId"));
                            }
                            initiated_by_inbox_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::AddedInboxes => {
                            if added_inboxes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("addedInboxes"));
                            }
                            added_inboxes__ = Some(map_.next_value()?);
                        }
                        GeneratedField::RemovedInboxes => {
                            if removed_inboxes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("removedInboxes"));
                            }
                            removed_inboxes__ = Some(map_.next_value()?);
                        }
                        GeneratedField::MetadataFieldChanges => {
                            if metadata_field_changes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("metadataFieldChanges"));
                            }
                            metadata_field_changes__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(GroupUpdated {
                    initiated_by_inbox_id: initiated_by_inbox_id__.unwrap_or_default(),
                    added_inboxes: added_inboxes__.unwrap_or_default(),
                    removed_inboxes: removed_inboxes__.unwrap_or_default(),
                    metadata_field_changes: metadata_field_changes__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.GroupUpdated", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for group_updated::Inbox {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.inbox_id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.GroupUpdated.Inbox", len)?;
        if !self.inbox_id.is_empty() {
            struct_ser.serialize_field("inboxId", &self.inbox_id)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for group_updated::Inbox {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "inbox_id",
            "inboxId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InboxId,
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
                            "inboxId" | "inbox_id" => Ok(GeneratedField::InboxId),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = group_updated::Inbox;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.GroupUpdated.Inbox")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<group_updated::Inbox, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut inbox_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::InboxId => {
                            if inbox_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inboxId"));
                            }
                            inbox_id__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(group_updated::Inbox {
                    inbox_id: inbox_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.GroupUpdated.Inbox", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for group_updated::MetadataFieldChange {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.field_name.is_empty() {
            len += 1;
        }
        if self.old_value.is_some() {
            len += 1;
        }
        if self.new_value.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.GroupUpdated.MetadataFieldChange", len)?;
        if !self.field_name.is_empty() {
            struct_ser.serialize_field("fieldName", &self.field_name)?;
        }
        if let Some(v) = self.old_value.as_ref() {
            struct_ser.serialize_field("oldValue", v)?;
        }
        if let Some(v) = self.new_value.as_ref() {
            struct_ser.serialize_field("newValue", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for group_updated::MetadataFieldChange {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "field_name",
            "fieldName",
            "old_value",
            "oldValue",
            "new_value",
            "newValue",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            FieldName,
            OldValue,
            NewValue,
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
                            "fieldName" | "field_name" => Ok(GeneratedField::FieldName),
                            "oldValue" | "old_value" => Ok(GeneratedField::OldValue),
                            "newValue" | "new_value" => Ok(GeneratedField::NewValue),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = group_updated::MetadataFieldChange;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.GroupUpdated.MetadataFieldChange")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<group_updated::MetadataFieldChange, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut field_name__ = None;
                let mut old_value__ = None;
                let mut new_value__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::FieldName => {
                            if field_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fieldName"));
                            }
                            field_name__ = Some(map_.next_value()?);
                        }
                        GeneratedField::OldValue => {
                            if old_value__.is_some() {
                                return Err(serde::de::Error::duplicate_field("oldValue"));
                            }
                            old_value__ = map_.next_value()?;
                        }
                        GeneratedField::NewValue => {
                            if new_value__.is_some() {
                                return Err(serde::de::Error::duplicate_field("newValue"));
                            }
                            new_value__ = map_.next_value()?;
                        }
                    }
                }
                Ok(group_updated::MetadataFieldChange {
                    field_name: field_name__.unwrap_or_default(),
                    old_value: old_value__,
                    new_value: new_value__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.GroupUpdated.MetadataFieldChange", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Inbox {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.inbox_id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.Inbox", len)?;
        if !self.inbox_id.is_empty() {
            struct_ser.serialize_field("inboxId", &self.inbox_id)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Inbox {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "inbox_id",
            "inboxId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InboxId,
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
                            "inboxId" | "inbox_id" => Ok(GeneratedField::InboxId),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Inbox;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.Inbox")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<Inbox, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut inbox_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::InboxId => {
                            if inbox_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inboxId"));
                            }
                            inbox_id__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(Inbox {
                    inbox_id: inbox_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.Inbox", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Inboxes {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.inbox_ids.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.Inboxes", len)?;
        if !self.inbox_ids.is_empty() {
            struct_ser.serialize_field("inboxIds", &self.inbox_ids)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Inboxes {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "inbox_ids",
            "inboxIds",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InboxIds,
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
                            "inboxIds" | "inbox_ids" => Ok(GeneratedField::InboxIds),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Inboxes;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.Inboxes")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<Inboxes, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut inbox_ids__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::InboxIds => {
                            if inbox_ids__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inboxIds"));
                            }
                            inbox_ids__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(Inboxes {
                    inbox_ids: inbox_ids__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.Inboxes", FIELDS, GeneratedVisitor)
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
            Self::AllowIfAdminOrSuperAdmin => "BASE_POLICY_ALLOW_IF_ADMIN_OR_SUPER_ADMIN",
            Self::AllowIfSuperAdmin => "BASE_POLICY_ALLOW_IF_SUPER_ADMIN",
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
            "BASE_POLICY_ALLOW_IF_ADMIN_OR_SUPER_ADMIN",
            "BASE_POLICY_ALLOW_IF_SUPER_ADMIN",
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
                    "BASE_POLICY_ALLOW_IF_ADMIN_OR_SUPER_ADMIN" => Ok(membership_policy::BasePolicy::AllowIfAdminOrSuperAdmin),
                    "BASE_POLICY_ALLOW_IF_SUPER_ADMIN" => Ok(membership_policy::BasePolicy::AllowIfSuperAdmin),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for MessageHistoryKeyType {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.key.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.MessageHistoryKeyType", len)?;
        if let Some(v) = self.key.as_ref() {
            match v {
                message_history_key_type::Key::Chacha20Poly1305(v) => {
                    #[allow(clippy::needless_borrow)]
                    struct_ser.serialize_field("chacha20Poly1305", pbjson::private::base64::encode(&v).as_str())?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for MessageHistoryKeyType {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "chacha20_poly1305",
            "chacha20Poly1305",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Chacha20Poly1305,
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
                            "chacha20Poly1305" | "chacha20_poly1305" => Ok(GeneratedField::Chacha20Poly1305),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = MessageHistoryKeyType;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.MessageHistoryKeyType")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<MessageHistoryKeyType, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut key__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Chacha20Poly1305 => {
                            if key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("chacha20Poly1305"));
                            }
                            key__ = map_.next_value::<::std::option::Option<::pbjson::private::BytesDeserialize<_>>>()?.map(|x| message_history_key_type::Key::Chacha20Poly1305(x.0));
                        }
                    }
                }
                Ok(MessageHistoryKeyType {
                    key: key__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.MessageHistoryKeyType", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for MessageHistoryReply {
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
        if !self.url.is_empty() {
            len += 1;
        }
        if self.encryption_key.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.MessageHistoryReply", len)?;
        if !self.request_id.is_empty() {
            struct_ser.serialize_field("requestId", &self.request_id)?;
        }
        if !self.url.is_empty() {
            struct_ser.serialize_field("url", &self.url)?;
        }
        if let Some(v) = self.encryption_key.as_ref() {
            struct_ser.serialize_field("encryptionKey", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for MessageHistoryReply {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "request_id",
            "requestId",
            "url",
            "encryption_key",
            "encryptionKey",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            RequestId,
            Url,
            EncryptionKey,
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
                            "url" => Ok(GeneratedField::Url),
                            "encryptionKey" | "encryption_key" => Ok(GeneratedField::EncryptionKey),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = MessageHistoryReply;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.MessageHistoryReply")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<MessageHistoryReply, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut request_id__ = None;
                let mut url__ = None;
                let mut encryption_key__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::RequestId => {
                            if request_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("requestId"));
                            }
                            request_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Url => {
                            if url__.is_some() {
                                return Err(serde::de::Error::duplicate_field("url"));
                            }
                            url__ = Some(map_.next_value()?);
                        }
                        GeneratedField::EncryptionKey => {
                            if encryption_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("encryptionKey"));
                            }
                            encryption_key__ = map_.next_value()?;
                        }
                    }
                }
                Ok(MessageHistoryReply {
                    request_id: request_id__.unwrap_or_default(),
                    url: url__.unwrap_or_default(),
                    encryption_key: encryption_key__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.MessageHistoryReply", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for MessageHistoryRequest {
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
        if !self.pin_code.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.MessageHistoryRequest", len)?;
        if !self.request_id.is_empty() {
            struct_ser.serialize_field("requestId", &self.request_id)?;
        }
        if !self.pin_code.is_empty() {
            struct_ser.serialize_field("pinCode", &self.pin_code)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for MessageHistoryRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "request_id",
            "requestId",
            "pin_code",
            "pinCode",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            RequestId,
            PinCode,
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
                            "pinCode" | "pin_code" => Ok(GeneratedField::PinCode),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = MessageHistoryRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.MessageHistoryRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<MessageHistoryRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut request_id__ = None;
                let mut pin_code__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::RequestId => {
                            if request_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("requestId"));
                            }
                            request_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::PinCode => {
                            if pin_code__.is_some() {
                                return Err(serde::de::Error::duplicate_field("pinCode"));
                            }
                            pin_code__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(MessageHistoryRequest {
                    request_id: request_id__.unwrap_or_default(),
                    pin_code: pin_code__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.MessageHistoryRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for MetadataPolicy {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.MetadataPolicy", len)?;
        if let Some(v) = self.kind.as_ref() {
            match v {
                metadata_policy::Kind::Base(v) => {
                    let v = metadata_policy::MetadataBasePolicy::try_from(*v)
                        .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", *v)))?;
                    struct_ser.serialize_field("base", &v)?;
                }
                metadata_policy::Kind::AndCondition(v) => {
                    struct_ser.serialize_field("andCondition", v)?;
                }
                metadata_policy::Kind::AnyCondition(v) => {
                    struct_ser.serialize_field("anyCondition", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for MetadataPolicy {
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
            type Value = MetadataPolicy;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.MetadataPolicy")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<MetadataPolicy, V::Error>
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
                            kind__ = map_.next_value::<::std::option::Option<metadata_policy::MetadataBasePolicy>>()?.map(|x| metadata_policy::Kind::Base(x as i32));
                        }
                        GeneratedField::AndCondition => {
                            if kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("andCondition"));
                            }
                            kind__ = map_.next_value::<::std::option::Option<_>>()?.map(metadata_policy::Kind::AndCondition)
;
                        }
                        GeneratedField::AnyCondition => {
                            if kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("anyCondition"));
                            }
                            kind__ = map_.next_value::<::std::option::Option<_>>()?.map(metadata_policy::Kind::AnyCondition)
;
                        }
                    }
                }
                Ok(MetadataPolicy {
                    kind: kind__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.MetadataPolicy", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for metadata_policy::AndCondition {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.MetadataPolicy.AndCondition", len)?;
        if !self.policies.is_empty() {
            struct_ser.serialize_field("policies", &self.policies)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for metadata_policy::AndCondition {
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
            type Value = metadata_policy::AndCondition;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.MetadataPolicy.AndCondition")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<metadata_policy::AndCondition, V::Error>
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
                Ok(metadata_policy::AndCondition {
                    policies: policies__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.MetadataPolicy.AndCondition", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for metadata_policy::AnyCondition {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.MetadataPolicy.AnyCondition", len)?;
        if !self.policies.is_empty() {
            struct_ser.serialize_field("policies", &self.policies)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for metadata_policy::AnyCondition {
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
            type Value = metadata_policy::AnyCondition;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.MetadataPolicy.AnyCondition")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<metadata_policy::AnyCondition, V::Error>
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
                Ok(metadata_policy::AnyCondition {
                    policies: policies__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.MetadataPolicy.AnyCondition", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for metadata_policy::MetadataBasePolicy {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "METADATA_BASE_POLICY_UNSPECIFIED",
            Self::Allow => "METADATA_BASE_POLICY_ALLOW",
            Self::Deny => "METADATA_BASE_POLICY_DENY",
            Self::AllowIfAdmin => "METADATA_BASE_POLICY_ALLOW_IF_ADMIN",
            Self::AllowIfSuperAdmin => "METADATA_BASE_POLICY_ALLOW_IF_SUPER_ADMIN",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for metadata_policy::MetadataBasePolicy {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "METADATA_BASE_POLICY_UNSPECIFIED",
            "METADATA_BASE_POLICY_ALLOW",
            "METADATA_BASE_POLICY_DENY",
            "METADATA_BASE_POLICY_ALLOW_IF_ADMIN",
            "METADATA_BASE_POLICY_ALLOW_IF_SUPER_ADMIN",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = metadata_policy::MetadataBasePolicy;

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
                    "METADATA_BASE_POLICY_UNSPECIFIED" => Ok(metadata_policy::MetadataBasePolicy::Unspecified),
                    "METADATA_BASE_POLICY_ALLOW" => Ok(metadata_policy::MetadataBasePolicy::Allow),
                    "METADATA_BASE_POLICY_DENY" => Ok(metadata_policy::MetadataBasePolicy::Deny),
                    "METADATA_BASE_POLICY_ALLOW_IF_ADMIN" => Ok(metadata_policy::MetadataBasePolicy::AllowIfAdmin),
                    "METADATA_BASE_POLICY_ALLOW_IF_SUPER_ADMIN" => Ok(metadata_policy::MetadataBasePolicy::AllowIfSuperAdmin),
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
impl serde::Serialize for PermissionsUpdatePolicy {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.PermissionsUpdatePolicy", len)?;
        if let Some(v) = self.kind.as_ref() {
            match v {
                permissions_update_policy::Kind::Base(v) => {
                    let v = permissions_update_policy::PermissionsBasePolicy::try_from(*v)
                        .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", *v)))?;
                    struct_ser.serialize_field("base", &v)?;
                }
                permissions_update_policy::Kind::AndCondition(v) => {
                    struct_ser.serialize_field("andCondition", v)?;
                }
                permissions_update_policy::Kind::AnyCondition(v) => {
                    struct_ser.serialize_field("anyCondition", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for PermissionsUpdatePolicy {
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
            type Value = PermissionsUpdatePolicy;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.PermissionsUpdatePolicy")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<PermissionsUpdatePolicy, V::Error>
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
                            kind__ = map_.next_value::<::std::option::Option<permissions_update_policy::PermissionsBasePolicy>>()?.map(|x| permissions_update_policy::Kind::Base(x as i32));
                        }
                        GeneratedField::AndCondition => {
                            if kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("andCondition"));
                            }
                            kind__ = map_.next_value::<::std::option::Option<_>>()?.map(permissions_update_policy::Kind::AndCondition)
;
                        }
                        GeneratedField::AnyCondition => {
                            if kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("anyCondition"));
                            }
                            kind__ = map_.next_value::<::std::option::Option<_>>()?.map(permissions_update_policy::Kind::AnyCondition)
;
                        }
                    }
                }
                Ok(PermissionsUpdatePolicy {
                    kind: kind__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.PermissionsUpdatePolicy", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for permissions_update_policy::AndCondition {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.PermissionsUpdatePolicy.AndCondition", len)?;
        if !self.policies.is_empty() {
            struct_ser.serialize_field("policies", &self.policies)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for permissions_update_policy::AndCondition {
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
            type Value = permissions_update_policy::AndCondition;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.PermissionsUpdatePolicy.AndCondition")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<permissions_update_policy::AndCondition, V::Error>
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
                Ok(permissions_update_policy::AndCondition {
                    policies: policies__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.PermissionsUpdatePolicy.AndCondition", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for permissions_update_policy::AnyCondition {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.PermissionsUpdatePolicy.AnyCondition", len)?;
        if !self.policies.is_empty() {
            struct_ser.serialize_field("policies", &self.policies)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for permissions_update_policy::AnyCondition {
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
            type Value = permissions_update_policy::AnyCondition;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.PermissionsUpdatePolicy.AnyCondition")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<permissions_update_policy::AnyCondition, V::Error>
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
                Ok(permissions_update_policy::AnyCondition {
                    policies: policies__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.PermissionsUpdatePolicy.AnyCondition", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for permissions_update_policy::PermissionsBasePolicy {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "PERMISSIONS_BASE_POLICY_UNSPECIFIED",
            Self::Deny => "PERMISSIONS_BASE_POLICY_DENY",
            Self::AllowIfAdmin => "PERMISSIONS_BASE_POLICY_ALLOW_IF_ADMIN",
            Self::AllowIfSuperAdmin => "PERMISSIONS_BASE_POLICY_ALLOW_IF_SUPER_ADMIN",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for permissions_update_policy::PermissionsBasePolicy {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "PERMISSIONS_BASE_POLICY_UNSPECIFIED",
            "PERMISSIONS_BASE_POLICY_DENY",
            "PERMISSIONS_BASE_POLICY_ALLOW_IF_ADMIN",
            "PERMISSIONS_BASE_POLICY_ALLOW_IF_SUPER_ADMIN",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = permissions_update_policy::PermissionsBasePolicy;

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
                    "PERMISSIONS_BASE_POLICY_UNSPECIFIED" => Ok(permissions_update_policy::PermissionsBasePolicy::Unspecified),
                    "PERMISSIONS_BASE_POLICY_DENY" => Ok(permissions_update_policy::PermissionsBasePolicy::Deny),
                    "PERMISSIONS_BASE_POLICY_ALLOW_IF_ADMIN" => Ok(permissions_update_policy::PermissionsBasePolicy::AllowIfAdmin),
                    "PERMISSIONS_BASE_POLICY_ALLOW_IF_SUPER_ADMIN" => Ok(permissions_update_policy::PermissionsBasePolicy::AllowIfSuperAdmin),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
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
                plaintext_envelope::Content::V2(v) => {
                    struct_ser.serialize_field("v2", v)?;
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
            "v2",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            V1,
            V2,
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
                            "v2" => Ok(GeneratedField::V2),
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
                        GeneratedField::V2 => {
                            if content__.is_some() {
                                return Err(serde::de::Error::duplicate_field("v2"));
                            }
                            content__ = map_.next_value::<::std::option::Option<_>>()?.map(plaintext_envelope::Content::V2)
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
impl serde::Serialize for plaintext_envelope::V2 {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.idempotency_key.is_empty() {
            len += 1;
        }
        if self.message_type.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.PlaintextEnvelope.V2", len)?;
        if !self.idempotency_key.is_empty() {
            struct_ser.serialize_field("idempotencyKey", &self.idempotency_key)?;
        }
        if let Some(v) = self.message_type.as_ref() {
            match v {
                plaintext_envelope::v2::MessageType::Content(v) => {
                    #[allow(clippy::needless_borrow)]
                    struct_ser.serialize_field("content", pbjson::private::base64::encode(&v).as_str())?;
                }
                plaintext_envelope::v2::MessageType::Request(v) => {
                    struct_ser.serialize_field("request", v)?;
                }
                plaintext_envelope::v2::MessageType::Reply(v) => {
                    struct_ser.serialize_field("reply", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for plaintext_envelope::V2 {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "idempotency_key",
            "idempotencyKey",
            "content",
            "request",
            "reply",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            IdempotencyKey,
            Content,
            Request,
            Reply,
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
                            "idempotencyKey" | "idempotency_key" => Ok(GeneratedField::IdempotencyKey),
                            "content" => Ok(GeneratedField::Content),
                            "request" => Ok(GeneratedField::Request),
                            "reply" => Ok(GeneratedField::Reply),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = plaintext_envelope::V2;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.message_contents.PlaintextEnvelope.V2")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<plaintext_envelope::V2, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut idempotency_key__ = None;
                let mut message_type__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::IdempotencyKey => {
                            if idempotency_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("idempotencyKey"));
                            }
                            idempotency_key__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Content => {
                            if message_type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("content"));
                            }
                            message_type__ = map_.next_value::<::std::option::Option<::pbjson::private::BytesDeserialize<_>>>()?.map(|x| plaintext_envelope::v2::MessageType::Content(x.0));
                        }
                        GeneratedField::Request => {
                            if message_type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("request"));
                            }
                            message_type__ = map_.next_value::<::std::option::Option<_>>()?.map(plaintext_envelope::v2::MessageType::Request)
;
                        }
                        GeneratedField::Reply => {
                            if message_type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("reply"));
                            }
                            message_type__ = map_.next_value::<::std::option::Option<_>>()?.map(plaintext_envelope::v2::MessageType::Reply)
;
                        }
                    }
                }
                Ok(plaintext_envelope::V2 {
                    idempotency_key: idempotency_key__.unwrap_or_default(),
                    message_type: message_type__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.message_contents.PlaintextEnvelope.V2", FIELDS, GeneratedVisitor)
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
        if !self.update_metadata_policy.is_empty() {
            len += 1;
        }
        if self.add_admin_policy.is_some() {
            len += 1;
        }
        if self.remove_admin_policy.is_some() {
            len += 1;
        }
        if self.update_permissions_policy.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.message_contents.PolicySet", len)?;
        if let Some(v) = self.add_member_policy.as_ref() {
            struct_ser.serialize_field("addMemberPolicy", v)?;
        }
        if let Some(v) = self.remove_member_policy.as_ref() {
            struct_ser.serialize_field("removeMemberPolicy", v)?;
        }
        if !self.update_metadata_policy.is_empty() {
            struct_ser.serialize_field("updateMetadataPolicy", &self.update_metadata_policy)?;
        }
        if let Some(v) = self.add_admin_policy.as_ref() {
            struct_ser.serialize_field("addAdminPolicy", v)?;
        }
        if let Some(v) = self.remove_admin_policy.as_ref() {
            struct_ser.serialize_field("removeAdminPolicy", v)?;
        }
        if let Some(v) = self.update_permissions_policy.as_ref() {
            struct_ser.serialize_field("updatePermissionsPolicy", v)?;
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
            "update_metadata_policy",
            "updateMetadataPolicy",
            "add_admin_policy",
            "addAdminPolicy",
            "remove_admin_policy",
            "removeAdminPolicy",
            "update_permissions_policy",
            "updatePermissionsPolicy",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AddMemberPolicy,
            RemoveMemberPolicy,
            UpdateMetadataPolicy,
            AddAdminPolicy,
            RemoveAdminPolicy,
            UpdatePermissionsPolicy,
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
                            "updateMetadataPolicy" | "update_metadata_policy" => Ok(GeneratedField::UpdateMetadataPolicy),
                            "addAdminPolicy" | "add_admin_policy" => Ok(GeneratedField::AddAdminPolicy),
                            "removeAdminPolicy" | "remove_admin_policy" => Ok(GeneratedField::RemoveAdminPolicy),
                            "updatePermissionsPolicy" | "update_permissions_policy" => Ok(GeneratedField::UpdatePermissionsPolicy),
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
                let mut update_metadata_policy__ = None;
                let mut add_admin_policy__ = None;
                let mut remove_admin_policy__ = None;
                let mut update_permissions_policy__ = None;
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
                        GeneratedField::UpdateMetadataPolicy => {
                            if update_metadata_policy__.is_some() {
                                return Err(serde::de::Error::duplicate_field("updateMetadataPolicy"));
                            }
                            update_metadata_policy__ = Some(
                                map_.next_value::<std::collections::HashMap<_, _>>()?
                            );
                        }
                        GeneratedField::AddAdminPolicy => {
                            if add_admin_policy__.is_some() {
                                return Err(serde::de::Error::duplicate_field("addAdminPolicy"));
                            }
                            add_admin_policy__ = map_.next_value()?;
                        }
                        GeneratedField::RemoveAdminPolicy => {
                            if remove_admin_policy__.is_some() {
                                return Err(serde::de::Error::duplicate_field("removeAdminPolicy"));
                            }
                            remove_admin_policy__ = map_.next_value()?;
                        }
                        GeneratedField::UpdatePermissionsPolicy => {
                            if update_permissions_policy__.is_some() {
                                return Err(serde::de::Error::duplicate_field("updatePermissionsPolicy"));
                            }
                            update_permissions_policy__ = map_.next_value()?;
                        }
                    }
                }
                Ok(PolicySet {
                    add_member_policy: add_member_policy__,
                    remove_member_policy: remove_member_policy__,
                    update_metadata_policy: update_metadata_policy__.unwrap_or_default(),
                    add_admin_policy: add_admin_policy__,
                    remove_admin_policy: remove_admin_policy__,
                    update_permissions_policy: update_permissions_policy__,
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
