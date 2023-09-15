// @generated
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
        let mut struct_ser = serializer.serialize_struct("xmtp.v3.message_contents.EdDsaSignature", len)?;
        if !self.bytes.is_empty() {
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
                formatter.write_str("struct xmtp.v3.message_contents.EdDsaSignature")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<EdDsaSignature, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut bytes__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Bytes => {
                            if bytes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("bytes"));
                            }
                            bytes__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(EdDsaSignature {
                    bytes: bytes__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.v3.message_contents.EdDsaSignature", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Eip191Association {
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
        if self.association_data.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.v3.message_contents.Eip191Association", len)?;
        if let Some(v) = self.signature.as_ref() {
            struct_ser.serialize_field("signature", v)?;
        }
        if let Some(v) = self.association_data.as_ref() {
            match v {
                eip191_association::AssociationData::CreateIdentityData(v) => {
                    struct_ser.serialize_field("createIdentityData", v)?;
                }
                eip191_association::AssociationData::InstallationGrantData(v) => {
                    struct_ser.serialize_field("installationGrantData", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Eip191Association {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "signature",
            "create_identity_data",
            "createIdentityData",
            "installation_grant_data",
            "installationGrantData",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Signature,
            CreateIdentityData,
            InstallationGrantData,
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
                            "createIdentityData" | "create_identity_data" => Ok(GeneratedField::CreateIdentityData),
                            "installationGrantData" | "installation_grant_data" => Ok(GeneratedField::InstallationGrantData),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Eip191Association;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.v3.message_contents.Eip191Association")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<Eip191Association, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut signature__ = None;
                let mut association_data__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Signature => {
                            if signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("signature"));
                            }
                            signature__ = map.next_value()?;
                        }
                        GeneratedField::CreateIdentityData => {
                            if association_data__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createIdentityData"));
                            }
                            association_data__ = map.next_value::<::std::option::Option<_>>()?.map(eip191_association::AssociationData::CreateIdentityData)
;
                        }
                        GeneratedField::InstallationGrantData => {
                            if association_data__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationGrantData"));
                            }
                            association_data__ = map.next_value::<::std::option::Option<_>>()?.map(eip191_association::AssociationData::InstallationGrantData)
;
                        }
                    }
                }
                Ok(Eip191Association {
                    signature: signature__,
                    association_data: association_data__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.v3.message_contents.Eip191Association", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for eip191_association::CreateIdentityAssociationData {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.wallet_address.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.v3.message_contents.Eip191Association.CreateIdentityAssociationData", len)?;
        if !self.wallet_address.is_empty() {
            struct_ser.serialize_field("walletAddress", &self.wallet_address)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for eip191_association::CreateIdentityAssociationData {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "wallet_address",
            "walletAddress",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            WalletAddress,
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
                            "walletAddress" | "wallet_address" => Ok(GeneratedField::WalletAddress),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = eip191_association::CreateIdentityAssociationData;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.v3.message_contents.Eip191Association.CreateIdentityAssociationData")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<eip191_association::CreateIdentityAssociationData, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut wallet_address__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::WalletAddress => {
                            if wallet_address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("walletAddress"));
                            }
                            wallet_address__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(eip191_association::CreateIdentityAssociationData {
                    wallet_address: wallet_address__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.v3.message_contents.Eip191Association.CreateIdentityAssociationData", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for eip191_association::InstallationGrantAssociationData {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.wallet_address.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.v3.message_contents.Eip191Association.InstallationGrantAssociationData", len)?;
        if !self.wallet_address.is_empty() {
            struct_ser.serialize_field("walletAddress", &self.wallet_address)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for eip191_association::InstallationGrantAssociationData {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "wallet_address",
            "walletAddress",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            WalletAddress,
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
                            "walletAddress" | "wallet_address" => Ok(GeneratedField::WalletAddress),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = eip191_association::InstallationGrantAssociationData;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.v3.message_contents.Eip191Association.InstallationGrantAssociationData")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<eip191_association::InstallationGrantAssociationData, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut wallet_address__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::WalletAddress => {
                            if wallet_address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("walletAddress"));
                            }
                            wallet_address__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(eip191_association::InstallationGrantAssociationData {
                    wallet_address: wallet_address__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.v3.message_contents.Eip191Association.InstallationGrantAssociationData", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for IdentityKeyAssociation {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.identity_signature.is_some() {
            len += 1;
        }
        if !self.identity_key_bytes.is_empty() {
            len += 1;
        }
        if self.identity_key_wallet_association.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.v3.message_contents.IdentityKeyAssociation", len)?;
        if let Some(v) = self.identity_signature.as_ref() {
            struct_ser.serialize_field("identitySignature", v)?;
        }
        if !self.identity_key_bytes.is_empty() {
            struct_ser.serialize_field("identityKeyBytes", pbjson::private::base64::encode(&self.identity_key_bytes).as_str())?;
        }
        if let Some(v) = self.identity_key_wallet_association.as_ref() {
            struct_ser.serialize_field("identityKeyWalletAssociation", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for IdentityKeyAssociation {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "identity_signature",
            "identitySignature",
            "identity_key_bytes",
            "identityKeyBytes",
            "identity_key_wallet_association",
            "identityKeyWalletAssociation",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            IdentitySignature,
            IdentityKeyBytes,
            IdentityKeyWalletAssociation,
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
                            "identitySignature" | "identity_signature" => Ok(GeneratedField::IdentitySignature),
                            "identityKeyBytes" | "identity_key_bytes" => Ok(GeneratedField::IdentityKeyBytes),
                            "identityKeyWalletAssociation" | "identity_key_wallet_association" => Ok(GeneratedField::IdentityKeyWalletAssociation),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = IdentityKeyAssociation;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.v3.message_contents.IdentityKeyAssociation")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<IdentityKeyAssociation, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut identity_signature__ = None;
                let mut identity_key_bytes__ = None;
                let mut identity_key_wallet_association__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::IdentitySignature => {
                            if identity_signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("identitySignature"));
                            }
                            identity_signature__ = map.next_value()?;
                        }
                        GeneratedField::IdentityKeyBytes => {
                            if identity_key_bytes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("identityKeyBytes"));
                            }
                            identity_key_bytes__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::IdentityKeyWalletAssociation => {
                            if identity_key_wallet_association__.is_some() {
                                return Err(serde::de::Error::duplicate_field("identityKeyWalletAssociation"));
                            }
                            identity_key_wallet_association__ = map.next_value()?;
                        }
                    }
                }
                Ok(IdentityKeyAssociation {
                    identity_signature: identity_signature__,
                    identity_key_bytes: identity_key_bytes__.unwrap_or_default(),
                    identity_key_wallet_association: identity_key_wallet_association__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.v3.message_contents.IdentityKeyAssociation", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for InstallationContactBundle {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.v3.message_contents.InstallationContactBundle", len)?;
        if let Some(v) = self.version.as_ref() {
            match v {
                installation_contact_bundle::Version::V1(v) => {
                    struct_ser.serialize_field("v1", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for InstallationContactBundle {
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
            type Value = InstallationContactBundle;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.v3.message_contents.InstallationContactBundle")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<InstallationContactBundle, V::Error>
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
                            version__ = map.next_value::<::std::option::Option<_>>()?.map(installation_contact_bundle::Version::V1)
;
                        }
                    }
                }
                Ok(InstallationContactBundle {
                    version: version__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.v3.message_contents.InstallationContactBundle", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for InvitationEnvelope {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.v3.message_contents.InvitationEnvelope", len)?;
        if let Some(v) = self.version.as_ref() {
            match v {
                invitation_envelope::Version::V1(v) => {
                    struct_ser.serialize_field("v1", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for InvitationEnvelope {
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
            type Value = InvitationEnvelope;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.v3.message_contents.InvitationEnvelope")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<InvitationEnvelope, V::Error>
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
                            version__ = map.next_value::<::std::option::Option<_>>()?.map(invitation_envelope::Version::V1)
;
                        }
                    }
                }
                Ok(InvitationEnvelope {
                    version: version__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.v3.message_contents.InvitationEnvelope", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for InvitationEnvelopeV1 {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.inviter.is_some() {
            len += 1;
        }
        if !self.ciphertext.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.v3.message_contents.InvitationEnvelopeV1", len)?;
        if let Some(v) = self.inviter.as_ref() {
            struct_ser.serialize_field("inviter", v)?;
        }
        if !self.ciphertext.is_empty() {
            struct_ser.serialize_field("ciphertext", pbjson::private::base64::encode(&self.ciphertext).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for InvitationEnvelopeV1 {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "inviter",
            "ciphertext",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Inviter,
            Ciphertext,
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
                            "inviter" => Ok(GeneratedField::Inviter),
                            "ciphertext" => Ok(GeneratedField::Ciphertext),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = InvitationEnvelopeV1;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.v3.message_contents.InvitationEnvelopeV1")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<InvitationEnvelopeV1, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut inviter__ = None;
                let mut ciphertext__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Inviter => {
                            if inviter__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inviter"));
                            }
                            inviter__ = map.next_value()?;
                        }
                        GeneratedField::Ciphertext => {
                            if ciphertext__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ciphertext"));
                            }
                            ciphertext__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(InvitationEnvelopeV1 {
                    inviter: inviter__,
                    ciphertext: ciphertext__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.v3.message_contents.InvitationEnvelopeV1", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for InvitationV1 {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.invitee_wallet_address.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.v3.message_contents.InvitationV1", len)?;
        if !self.invitee_wallet_address.is_empty() {
            struct_ser.serialize_field("inviteeWalletAddress", &self.invitee_wallet_address)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for InvitationV1 {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "invitee_wallet_address",
            "inviteeWalletAddress",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InviteeWalletAddress,
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
                            "inviteeWalletAddress" | "invitee_wallet_address" => Ok(GeneratedField::InviteeWalletAddress),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = InvitationV1;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.v3.message_contents.InvitationV1")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<InvitationV1, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut invitee_wallet_address__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::InviteeWalletAddress => {
                            if invitee_wallet_address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inviteeWalletAddress"));
                            }
                            invitee_wallet_address__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(InvitationV1 {
                    invitee_wallet_address: invitee_wallet_address__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.v3.message_contents.InvitationV1", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for PadlockMessageEnvelope {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.header_bytes.is_empty() {
            len += 1;
        }
        if !self.ciphertext.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.v3.message_contents.PadlockMessageEnvelope", len)?;
        if !self.header_bytes.is_empty() {
            struct_ser.serialize_field("headerBytes", pbjson::private::base64::encode(&self.header_bytes).as_str())?;
        }
        if !self.ciphertext.is_empty() {
            struct_ser.serialize_field("ciphertext", pbjson::private::base64::encode(&self.ciphertext).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for PadlockMessageEnvelope {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "header_bytes",
            "headerBytes",
            "ciphertext",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            HeaderBytes,
            Ciphertext,
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
                            "headerBytes" | "header_bytes" => Ok(GeneratedField::HeaderBytes),
                            "ciphertext" => Ok(GeneratedField::Ciphertext),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = PadlockMessageEnvelope;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.v3.message_contents.PadlockMessageEnvelope")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<PadlockMessageEnvelope, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut header_bytes__ = None;
                let mut ciphertext__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::HeaderBytes => {
                            if header_bytes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("headerBytes"));
                            }
                            header_bytes__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Ciphertext => {
                            if ciphertext__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ciphertext"));
                            }
                            ciphertext__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(PadlockMessageEnvelope {
                    header_bytes: header_bytes__.unwrap_or_default(),
                    ciphertext: ciphertext__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.v3.message_contents.PadlockMessageEnvelope", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for PadlockMessageHeader {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.sent_ns != 0 {
            len += 1;
        }
        if !self.sealed_metadata.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.v3.message_contents.PadlockMessageHeader", len)?;
        if self.sent_ns != 0 {
            struct_ser.serialize_field("sentNs", ToString::to_string(&self.sent_ns).as_str())?;
        }
        if !self.sealed_metadata.is_empty() {
            struct_ser.serialize_field("sealedMetadata", pbjson::private::base64::encode(&self.sealed_metadata).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for PadlockMessageHeader {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "sent_ns",
            "sentNs",
            "sealed_metadata",
            "sealedMetadata",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            SentNs,
            SealedMetadata,
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
                            "sentNs" | "sent_ns" => Ok(GeneratedField::SentNs),
                            "sealedMetadata" | "sealed_metadata" => Ok(GeneratedField::SealedMetadata),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = PadlockMessageHeader;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.v3.message_contents.PadlockMessageHeader")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<PadlockMessageHeader, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut sent_ns__ = None;
                let mut sealed_metadata__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::SentNs => {
                            if sent_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sentNs"));
                            }
                            sent_ns__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::SealedMetadata => {
                            if sealed_metadata__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sealedMetadata"));
                            }
                            sealed_metadata__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(PadlockMessageHeader {
                    sent_ns: sent_ns__.unwrap_or_default(),
                    sealed_metadata: sealed_metadata__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.v3.message_contents.PadlockMessageHeader", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for PadlockMessagePayload {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.message_version != 0 {
            len += 1;
        }
        if self.header_signature.is_some() {
            len += 1;
        }
        if !self.convo_id.is_empty() {
            len += 1;
        }
        if !self.content_bytes.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.v3.message_contents.PadlockMessagePayload", len)?;
        if self.message_version != 0 {
            let v = PadlockMessagePayloadVersion::from_i32(self.message_version)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.message_version)))?;
            struct_ser.serialize_field("messageVersion", &v)?;
        }
        if let Some(v) = self.header_signature.as_ref() {
            struct_ser.serialize_field("headerSignature", v)?;
        }
        if !self.convo_id.is_empty() {
            struct_ser.serialize_field("convoId", &self.convo_id)?;
        }
        if !self.content_bytes.is_empty() {
            struct_ser.serialize_field("contentBytes", pbjson::private::base64::encode(&self.content_bytes).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for PadlockMessagePayload {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "message_version",
            "messageVersion",
            "header_signature",
            "headerSignature",
            "convo_id",
            "convoId",
            "content_bytes",
            "contentBytes",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            MessageVersion,
            HeaderSignature,
            ConvoId,
            ContentBytes,
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
                            "messageVersion" | "message_version" => Ok(GeneratedField::MessageVersion),
                            "headerSignature" | "header_signature" => Ok(GeneratedField::HeaderSignature),
                            "convoId" | "convo_id" => Ok(GeneratedField::ConvoId),
                            "contentBytes" | "content_bytes" => Ok(GeneratedField::ContentBytes),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = PadlockMessagePayload;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.v3.message_contents.PadlockMessagePayload")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<PadlockMessagePayload, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut message_version__ = None;
                let mut header_signature__ = None;
                let mut convo_id__ = None;
                let mut content_bytes__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::MessageVersion => {
                            if message_version__.is_some() {
                                return Err(serde::de::Error::duplicate_field("messageVersion"));
                            }
                            message_version__ = Some(map.next_value::<PadlockMessagePayloadVersion>()? as i32);
                        }
                        GeneratedField::HeaderSignature => {
                            if header_signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("headerSignature"));
                            }
                            header_signature__ = map.next_value()?;
                        }
                        GeneratedField::ConvoId => {
                            if convo_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("convoId"));
                            }
                            convo_id__ = Some(map.next_value()?);
                        }
                        GeneratedField::ContentBytes => {
                            if content_bytes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("contentBytes"));
                            }
                            content_bytes__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(PadlockMessagePayload {
                    message_version: message_version__.unwrap_or_default(),
                    header_signature: header_signature__,
                    convo_id: convo_id__.unwrap_or_default(),
                    content_bytes: content_bytes__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.v3.message_contents.PadlockMessagePayload", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for PadlockMessagePayloadVersion {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "PADLOCK_MESSAGE_PAYLOAD_VERSION_UNSPECIFIED",
            Self::One => "PADLOCK_MESSAGE_PAYLOAD_VERSION_ONE",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for PadlockMessagePayloadVersion {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "PADLOCK_MESSAGE_PAYLOAD_VERSION_UNSPECIFIED",
            "PADLOCK_MESSAGE_PAYLOAD_VERSION_ONE",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = PadlockMessagePayloadVersion;

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
                    .and_then(PadlockMessagePayloadVersion::from_i32)
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
                    .and_then(PadlockMessagePayloadVersion::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "PADLOCK_MESSAGE_PAYLOAD_VERSION_UNSPECIFIED" => Ok(PadlockMessagePayloadVersion::Unspecified),
                    "PADLOCK_MESSAGE_PAYLOAD_VERSION_ONE" => Ok(PadlockMessagePayloadVersion::One),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for PadlockMessageSealedMetadata {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.sender_user_address.is_empty() {
            len += 1;
        }
        if !self.sender_installation_id.is_empty() {
            len += 1;
        }
        if !self.recipient_user_address.is_empty() {
            len += 1;
        }
        if !self.recipient_installation_id.is_empty() {
            len += 1;
        }
        if self.is_prekey_message {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.v3.message_contents.PadlockMessageSealedMetadata", len)?;
        if !self.sender_user_address.is_empty() {
            struct_ser.serialize_field("senderUserAddress", &self.sender_user_address)?;
        }
        if !self.sender_installation_id.is_empty() {
            struct_ser.serialize_field("senderInstallationId", &self.sender_installation_id)?;
        }
        if !self.recipient_user_address.is_empty() {
            struct_ser.serialize_field("recipientUserAddress", &self.recipient_user_address)?;
        }
        if !self.recipient_installation_id.is_empty() {
            struct_ser.serialize_field("recipientInstallationId", &self.recipient_installation_id)?;
        }
        if self.is_prekey_message {
            struct_ser.serialize_field("isPrekeyMessage", &self.is_prekey_message)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for PadlockMessageSealedMetadata {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "sender_user_address",
            "senderUserAddress",
            "sender_installation_id",
            "senderInstallationId",
            "recipient_user_address",
            "recipientUserAddress",
            "recipient_installation_id",
            "recipientInstallationId",
            "is_prekey_message",
            "isPrekeyMessage",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            SenderUserAddress,
            SenderInstallationId,
            RecipientUserAddress,
            RecipientInstallationId,
            IsPrekeyMessage,
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
                            "senderUserAddress" | "sender_user_address" => Ok(GeneratedField::SenderUserAddress),
                            "senderInstallationId" | "sender_installation_id" => Ok(GeneratedField::SenderInstallationId),
                            "recipientUserAddress" | "recipient_user_address" => Ok(GeneratedField::RecipientUserAddress),
                            "recipientInstallationId" | "recipient_installation_id" => Ok(GeneratedField::RecipientInstallationId),
                            "isPrekeyMessage" | "is_prekey_message" => Ok(GeneratedField::IsPrekeyMessage),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = PadlockMessageSealedMetadata;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.v3.message_contents.PadlockMessageSealedMetadata")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<PadlockMessageSealedMetadata, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut sender_user_address__ = None;
                let mut sender_installation_id__ = None;
                let mut recipient_user_address__ = None;
                let mut recipient_installation_id__ = None;
                let mut is_prekey_message__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::SenderUserAddress => {
                            if sender_user_address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("senderUserAddress"));
                            }
                            sender_user_address__ = Some(map.next_value()?);
                        }
                        GeneratedField::SenderInstallationId => {
                            if sender_installation_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("senderInstallationId"));
                            }
                            sender_installation_id__ = Some(map.next_value()?);
                        }
                        GeneratedField::RecipientUserAddress => {
                            if recipient_user_address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("recipientUserAddress"));
                            }
                            recipient_user_address__ = Some(map.next_value()?);
                        }
                        GeneratedField::RecipientInstallationId => {
                            if recipient_installation_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("recipientInstallationId"));
                            }
                            recipient_installation_id__ = Some(map.next_value()?);
                        }
                        GeneratedField::IsPrekeyMessage => {
                            if is_prekey_message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isPrekeyMessage"));
                            }
                            is_prekey_message__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(PadlockMessageSealedMetadata {
                    sender_user_address: sender_user_address__.unwrap_or_default(),
                    sender_installation_id: sender_installation_id__.unwrap_or_default(),
                    recipient_user_address: recipient_user_address__.unwrap_or_default(),
                    recipient_installation_id: recipient_installation_id__.unwrap_or_default(),
                    is_prekey_message: is_prekey_message__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.v3.message_contents.PadlockMessageSealedMetadata", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("xmtp.v3.message_contents.RecoverableEcdsaSignature", len)?;
        if !self.bytes.is_empty() {
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
                formatter.write_str("struct xmtp.v3.message_contents.RecoverableEcdsaSignature")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<RecoverableEcdsaSignature, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut bytes__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Bytes => {
                            if bytes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("bytes"));
                            }
                            bytes__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(RecoverableEcdsaSignature {
                    bytes: bytes__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.v3.message_contents.RecoverableEcdsaSignature", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for VmacAccountLinkedKey {
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
        if self.association.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.v3.message_contents.VmacAccountLinkedKey", len)?;
        if let Some(v) = self.key.as_ref() {
            struct_ser.serialize_field("key", v)?;
        }
        if let Some(v) = self.association.as_ref() {
            match v {
                vmac_account_linked_key::Association::Eip191(v) => {
                    struct_ser.serialize_field("eip191", v)?;
                }
                vmac_account_linked_key::Association::IdentityKeyAssociation(v) => {
                    struct_ser.serialize_field("identityKeyAssociation", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for VmacAccountLinkedKey {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "key",
            "eip_191",
            "eip191",
            "identity_key_association",
            "identityKeyAssociation",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Key,
            Eip191,
            IdentityKeyAssociation,
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
                            "eip191" | "eip_191" => Ok(GeneratedField::Eip191),
                            "identityKeyAssociation" | "identity_key_association" => Ok(GeneratedField::IdentityKeyAssociation),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = VmacAccountLinkedKey;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.v3.message_contents.VmacAccountLinkedKey")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<VmacAccountLinkedKey, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut key__ = None;
                let mut association__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Key => {
                            if key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("key"));
                            }
                            key__ = map.next_value()?;
                        }
                        GeneratedField::Eip191 => {
                            if association__.is_some() {
                                return Err(serde::de::Error::duplicate_field("eip191"));
                            }
                            association__ = map.next_value::<::std::option::Option<_>>()?.map(vmac_account_linked_key::Association::Eip191)
;
                        }
                        GeneratedField::IdentityKeyAssociation => {
                            if association__.is_some() {
                                return Err(serde::de::Error::duplicate_field("identityKeyAssociation"));
                            }
                            association__ = map.next_value::<::std::option::Option<_>>()?.map(vmac_account_linked_key::Association::IdentityKeyAssociation)
;
                        }
                    }
                }
                Ok(VmacAccountLinkedKey {
                    key: key__,
                    association: association__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.v3.message_contents.VmacAccountLinkedKey", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for VmacFallbackKeyRotation {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.identity_key.is_some() {
            len += 1;
        }
        if self.fallback_key.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.v3.message_contents.VmacFallbackKeyRotation", len)?;
        if let Some(v) = self.identity_key.as_ref() {
            struct_ser.serialize_field("identityKey", v)?;
        }
        if let Some(v) = self.fallback_key.as_ref() {
            struct_ser.serialize_field("fallbackKey", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for VmacFallbackKeyRotation {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "identity_key",
            "identityKey",
            "fallback_key",
            "fallbackKey",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            IdentityKey,
            FallbackKey,
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
                            "identityKey" | "identity_key" => Ok(GeneratedField::IdentityKey),
                            "fallbackKey" | "fallback_key" => Ok(GeneratedField::FallbackKey),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = VmacFallbackKeyRotation;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.v3.message_contents.VmacFallbackKeyRotation")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<VmacFallbackKeyRotation, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut identity_key__ = None;
                let mut fallback_key__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::IdentityKey => {
                            if identity_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("identityKey"));
                            }
                            identity_key__ = map.next_value()?;
                        }
                        GeneratedField::FallbackKey => {
                            if fallback_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fallbackKey"));
                            }
                            fallback_key__ = map.next_value()?;
                        }
                    }
                }
                Ok(VmacFallbackKeyRotation {
                    identity_key: identity_key__,
                    fallback_key: fallback_key__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.v3.message_contents.VmacFallbackKeyRotation", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for VmacInstallationLinkedKey {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.v3.message_contents.VmacInstallationLinkedKey", len)?;
        if let Some(v) = self.key.as_ref() {
            struct_ser.serialize_field("key", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for VmacInstallationLinkedKey {
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
            type Value = VmacInstallationLinkedKey;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.v3.message_contents.VmacInstallationLinkedKey")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<VmacInstallationLinkedKey, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut key__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Key => {
                            if key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("key"));
                            }
                            key__ = map.next_value()?;
                        }
                    }
                }
                Ok(VmacInstallationLinkedKey {
                    key: key__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.v3.message_contents.VmacInstallationLinkedKey", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for VmacInstallationPublicKeyBundleV1 {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.identity_key.is_some() {
            len += 1;
        }
        if self.fallback_key.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.v3.message_contents.VmacInstallationPublicKeyBundleV1", len)?;
        if let Some(v) = self.identity_key.as_ref() {
            struct_ser.serialize_field("identityKey", v)?;
        }
        if let Some(v) = self.fallback_key.as_ref() {
            struct_ser.serialize_field("fallbackKey", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for VmacInstallationPublicKeyBundleV1 {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "identity_key",
            "identityKey",
            "fallback_key",
            "fallbackKey",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            IdentityKey,
            FallbackKey,
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
                            "identityKey" | "identity_key" => Ok(GeneratedField::IdentityKey),
                            "fallbackKey" | "fallback_key" => Ok(GeneratedField::FallbackKey),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = VmacInstallationPublicKeyBundleV1;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.v3.message_contents.VmacInstallationPublicKeyBundleV1")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<VmacInstallationPublicKeyBundleV1, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut identity_key__ = None;
                let mut fallback_key__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::IdentityKey => {
                            if identity_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("identityKey"));
                            }
                            identity_key__ = map.next_value()?;
                        }
                        GeneratedField::FallbackKey => {
                            if fallback_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fallbackKey"));
                            }
                            fallback_key__ = map.next_value()?;
                        }
                    }
                }
                Ok(VmacInstallationPublicKeyBundleV1 {
                    identity_key: identity_key__,
                    fallback_key: fallback_key__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.v3.message_contents.VmacInstallationPublicKeyBundleV1", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for VmacOneTimeKeyTopupBundle {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.identity_key.is_some() {
            len += 1;
        }
        if !self.one_time_keys.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.v3.message_contents.VmacOneTimeKeyTopupBundle", len)?;
        if let Some(v) = self.identity_key.as_ref() {
            struct_ser.serialize_field("identityKey", v)?;
        }
        if !self.one_time_keys.is_empty() {
            struct_ser.serialize_field("oneTimeKeys", &self.one_time_keys)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for VmacOneTimeKeyTopupBundle {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "identity_key",
            "identityKey",
            "one_time_keys",
            "oneTimeKeys",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            IdentityKey,
            OneTimeKeys,
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
                            "identityKey" | "identity_key" => Ok(GeneratedField::IdentityKey),
                            "oneTimeKeys" | "one_time_keys" => Ok(GeneratedField::OneTimeKeys),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = VmacOneTimeKeyTopupBundle;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.v3.message_contents.VmacOneTimeKeyTopupBundle")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<VmacOneTimeKeyTopupBundle, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut identity_key__ = None;
                let mut one_time_keys__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::IdentityKey => {
                            if identity_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("identityKey"));
                            }
                            identity_key__ = map.next_value()?;
                        }
                        GeneratedField::OneTimeKeys => {
                            if one_time_keys__.is_some() {
                                return Err(serde::de::Error::duplicate_field("oneTimeKeys"));
                            }
                            one_time_keys__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(VmacOneTimeKeyTopupBundle {
                    identity_key: identity_key__,
                    one_time_keys: one_time_keys__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.v3.message_contents.VmacOneTimeKeyTopupBundle", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for VmacUnsignedPublicKey {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.created_ns != 0 {
            len += 1;
        }
        if self.union.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.v3.message_contents.VmacUnsignedPublicKey", len)?;
        if self.created_ns != 0 {
            struct_ser.serialize_field("createdNs", ToString::to_string(&self.created_ns).as_str())?;
        }
        if let Some(v) = self.union.as_ref() {
            match v {
                vmac_unsigned_public_key::Union::Curve25519(v) => {
                    struct_ser.serialize_field("curve25519", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for VmacUnsignedPublicKey {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "created_ns",
            "createdNs",
            "curve25519",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            CreatedNs,
            Curve25519,
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
                            "createdNs" | "created_ns" => Ok(GeneratedField::CreatedNs),
                            "curve25519" => Ok(GeneratedField::Curve25519),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = VmacUnsignedPublicKey;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.v3.message_contents.VmacUnsignedPublicKey")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<VmacUnsignedPublicKey, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut created_ns__ = None;
                let mut union__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::CreatedNs => {
                            if created_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createdNs"));
                            }
                            created_ns__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Curve25519 => {
                            if union__.is_some() {
                                return Err(serde::de::Error::duplicate_field("curve25519"));
                            }
                            union__ = map.next_value::<::std::option::Option<_>>()?.map(vmac_unsigned_public_key::Union::Curve25519)
;
                        }
                    }
                }
                Ok(VmacUnsignedPublicKey {
                    created_ns: created_ns__.unwrap_or_default(),
                    union: union__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.v3.message_contents.VmacUnsignedPublicKey", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for vmac_unsigned_public_key::VodozemacCurve25519 {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.v3.message_contents.VmacUnsignedPublicKey.VodozemacCurve25519", len)?;
        if !self.bytes.is_empty() {
            struct_ser.serialize_field("bytes", pbjson::private::base64::encode(&self.bytes).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for vmac_unsigned_public_key::VodozemacCurve25519 {
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
            type Value = vmac_unsigned_public_key::VodozemacCurve25519;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.v3.message_contents.VmacUnsignedPublicKey.VodozemacCurve25519")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<vmac_unsigned_public_key::VodozemacCurve25519, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut bytes__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Bytes => {
                            if bytes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("bytes"));
                            }
                            bytes__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(vmac_unsigned_public_key::VodozemacCurve25519 {
                    bytes: bytes__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.v3.message_contents.VmacUnsignedPublicKey.VodozemacCurve25519", FIELDS, GeneratedVisitor)
    }
}
