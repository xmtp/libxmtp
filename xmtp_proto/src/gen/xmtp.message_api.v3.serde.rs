// @generated
impl serde::Serialize for FetchKeyPackagesRequest {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.message_api.v3.FetchKeyPackagesRequest", len)?;
        if !self.installation_ids.is_empty() {
            struct_ser.serialize_field("installationIds", &self.installation_ids.iter().map(pbjson::private::base64::encode).collect::<Vec<_>>())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for FetchKeyPackagesRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "installation_ids",
            "installationIds",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InstallationIds,
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
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = FetchKeyPackagesRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.message_api.v3.FetchKeyPackagesRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<FetchKeyPackagesRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut installation_ids__ = None;
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
                    }
                }
                Ok(FetchKeyPackagesRequest {
                    installation_ids: installation_ids__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.message_api.v3.FetchKeyPackagesRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for FetchKeyPackagesResponse {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.message_api.v3.FetchKeyPackagesResponse", len)?;
        if !self.key_packages.is_empty() {
            struct_ser.serialize_field("keyPackages", &self.key_packages)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for FetchKeyPackagesResponse {
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
            type Value = FetchKeyPackagesResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.message_api.v3.FetchKeyPackagesResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<FetchKeyPackagesResponse, V::Error>
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
                Ok(FetchKeyPackagesResponse {
                    key_packages: key_packages__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.message_api.v3.FetchKeyPackagesResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for fetch_key_packages_response::KeyPackage {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.key_package_tls_serialized.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.message_api.v3.FetchKeyPackagesResponse.KeyPackage", len)?;
        if !self.key_package_tls_serialized.is_empty() {
            struct_ser.serialize_field("keyPackageTlsSerialized", pbjson::private::base64::encode(&self.key_package_tls_serialized).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for fetch_key_packages_response::KeyPackage {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "key_package_tls_serialized",
            "keyPackageTlsSerialized",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            KeyPackageTlsSerialized,
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
                            "keyPackageTlsSerialized" | "key_package_tls_serialized" => Ok(GeneratedField::KeyPackageTlsSerialized),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = fetch_key_packages_response::KeyPackage;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.message_api.v3.FetchKeyPackagesResponse.KeyPackage")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<fetch_key_packages_response::KeyPackage, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut key_package_tls_serialized__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::KeyPackageTlsSerialized => {
                            if key_package_tls_serialized__.is_some() {
                                return Err(serde::de::Error::duplicate_field("keyPackageTlsSerialized"));
                            }
                            key_package_tls_serialized__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(fetch_key_packages_response::KeyPackage {
                    key_package_tls_serialized: key_package_tls_serialized__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.message_api.v3.FetchKeyPackagesResponse.KeyPackage", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GetIdentityUpdatesRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.account_addresses.is_empty() {
            len += 1;
        }
        if self.start_time_ns != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.message_api.v3.GetIdentityUpdatesRequest", len)?;
        if !self.account_addresses.is_empty() {
            struct_ser.serialize_field("accountAddresses", &self.account_addresses)?;
        }
        if self.start_time_ns != 0 {
            struct_ser.serialize_field("startTimeNs", ToString::to_string(&self.start_time_ns).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GetIdentityUpdatesRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "account_addresses",
            "accountAddresses",
            "start_time_ns",
            "startTimeNs",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AccountAddresses,
            StartTimeNs,
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
                            "accountAddresses" | "account_addresses" => Ok(GeneratedField::AccountAddresses),
                            "startTimeNs" | "start_time_ns" => Ok(GeneratedField::StartTimeNs),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GetIdentityUpdatesRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.message_api.v3.GetIdentityUpdatesRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<GetIdentityUpdatesRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut account_addresses__ = None;
                let mut start_time_ns__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::AccountAddresses => {
                            if account_addresses__.is_some() {
                                return Err(serde::de::Error::duplicate_field("accountAddresses"));
                            }
                            account_addresses__ = Some(map.next_value()?);
                        }
                        GeneratedField::StartTimeNs => {
                            if start_time_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("startTimeNs"));
                            }
                            start_time_ns__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(GetIdentityUpdatesRequest {
                    account_addresses: account_addresses__.unwrap_or_default(),
                    start_time_ns: start_time_ns__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.message_api.v3.GetIdentityUpdatesRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GetIdentityUpdatesResponse {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.message_api.v3.GetIdentityUpdatesResponse", len)?;
        if !self.updates.is_empty() {
            struct_ser.serialize_field("updates", &self.updates)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GetIdentityUpdatesResponse {
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
            type Value = GetIdentityUpdatesResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.message_api.v3.GetIdentityUpdatesResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<GetIdentityUpdatesResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut updates__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Updates => {
                            if updates__.is_some() {
                                return Err(serde::de::Error::duplicate_field("updates"));
                            }
                            updates__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(GetIdentityUpdatesResponse {
                    updates: updates__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.message_api.v3.GetIdentityUpdatesResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for get_identity_updates_response::NewInstallationUpdate {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.installation_id.is_empty() {
            len += 1;
        }
        if !self.credential_identity.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.message_api.v3.GetIdentityUpdatesResponse.NewInstallationUpdate", len)?;
        if !self.installation_id.is_empty() {
            struct_ser.serialize_field("installationId", pbjson::private::base64::encode(&self.installation_id).as_str())?;
        }
        if !self.credential_identity.is_empty() {
            struct_ser.serialize_field("credentialIdentity", pbjson::private::base64::encode(&self.credential_identity).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for get_identity_updates_response::NewInstallationUpdate {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "installation_id",
            "installationId",
            "credential_identity",
            "credentialIdentity",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InstallationId,
            CredentialIdentity,
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
                            "installationId" | "installation_id" => Ok(GeneratedField::InstallationId),
                            "credentialIdentity" | "credential_identity" => Ok(GeneratedField::CredentialIdentity),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = get_identity_updates_response::NewInstallationUpdate;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.message_api.v3.GetIdentityUpdatesResponse.NewInstallationUpdate")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<get_identity_updates_response::NewInstallationUpdate, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut installation_id__ = None;
                let mut credential_identity__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::InstallationId => {
                            if installation_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationId"));
                            }
                            installation_id__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::CredentialIdentity => {
                            if credential_identity__.is_some() {
                                return Err(serde::de::Error::duplicate_field("credentialIdentity"));
                            }
                            credential_identity__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(get_identity_updates_response::NewInstallationUpdate {
                    installation_id: installation_id__.unwrap_or_default(),
                    credential_identity: credential_identity__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.message_api.v3.GetIdentityUpdatesResponse.NewInstallationUpdate", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for get_identity_updates_response::RevokedInstallationUpdate {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.installation_id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.message_api.v3.GetIdentityUpdatesResponse.RevokedInstallationUpdate", len)?;
        if !self.installation_id.is_empty() {
            struct_ser.serialize_field("installationId", pbjson::private::base64::encode(&self.installation_id).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for get_identity_updates_response::RevokedInstallationUpdate {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "installation_id",
            "installationId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InstallationId,
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
                            "installationId" | "installation_id" => Ok(GeneratedField::InstallationId),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = get_identity_updates_response::RevokedInstallationUpdate;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.message_api.v3.GetIdentityUpdatesResponse.RevokedInstallationUpdate")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<get_identity_updates_response::RevokedInstallationUpdate, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut installation_id__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::InstallationId => {
                            if installation_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationId"));
                            }
                            installation_id__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(get_identity_updates_response::RevokedInstallationUpdate {
                    installation_id: installation_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.message_api.v3.GetIdentityUpdatesResponse.RevokedInstallationUpdate", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for get_identity_updates_response::Update {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.timestamp_ns != 0 {
            len += 1;
        }
        if self.kind.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.message_api.v3.GetIdentityUpdatesResponse.Update", len)?;
        if self.timestamp_ns != 0 {
            struct_ser.serialize_field("timestampNs", ToString::to_string(&self.timestamp_ns).as_str())?;
        }
        if let Some(v) = self.kind.as_ref() {
            match v {
                get_identity_updates_response::update::Kind::NewInstallation(v) => {
                    struct_ser.serialize_field("newInstallation", v)?;
                }
                get_identity_updates_response::update::Kind::RevokedInstallation(v) => {
                    struct_ser.serialize_field("revokedInstallation", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for get_identity_updates_response::Update {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "timestamp_ns",
            "timestampNs",
            "new_installation",
            "newInstallation",
            "revoked_installation",
            "revokedInstallation",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            TimestampNs,
            NewInstallation,
            RevokedInstallation,
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
                            "timestampNs" | "timestamp_ns" => Ok(GeneratedField::TimestampNs),
                            "newInstallation" | "new_installation" => Ok(GeneratedField::NewInstallation),
                            "revokedInstallation" | "revoked_installation" => Ok(GeneratedField::RevokedInstallation),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = get_identity_updates_response::Update;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.message_api.v3.GetIdentityUpdatesResponse.Update")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<get_identity_updates_response::Update, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut timestamp_ns__ = None;
                let mut kind__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::TimestampNs => {
                            if timestamp_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("timestampNs"));
                            }
                            timestamp_ns__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::NewInstallation => {
                            if kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("newInstallation"));
                            }
                            kind__ = map.next_value::<::std::option::Option<_>>()?.map(get_identity_updates_response::update::Kind::NewInstallation)
;
                        }
                        GeneratedField::RevokedInstallation => {
                            if kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("revokedInstallation"));
                            }
                            kind__ = map.next_value::<::std::option::Option<_>>()?.map(get_identity_updates_response::update::Kind::RevokedInstallation)
;
                        }
                    }
                }
                Ok(get_identity_updates_response::Update {
                    timestamp_ns: timestamp_ns__.unwrap_or_default(),
                    kind: kind__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.message_api.v3.GetIdentityUpdatesResponse.Update", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for get_identity_updates_response::WalletUpdates {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.message_api.v3.GetIdentityUpdatesResponse.WalletUpdates", len)?;
        if !self.updates.is_empty() {
            struct_ser.serialize_field("updates", &self.updates)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for get_identity_updates_response::WalletUpdates {
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
            type Value = get_identity_updates_response::WalletUpdates;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.message_api.v3.GetIdentityUpdatesResponse.WalletUpdates")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<get_identity_updates_response::WalletUpdates, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut updates__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Updates => {
                            if updates__.is_some() {
                                return Err(serde::de::Error::duplicate_field("updates"));
                            }
                            updates__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(get_identity_updates_response::WalletUpdates {
                    updates: updates__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.message_api.v3.GetIdentityUpdatesResponse.WalletUpdates", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for KeyPackageUpload {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.key_package_tls_serialized.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.message_api.v3.KeyPackageUpload", len)?;
        if !self.key_package_tls_serialized.is_empty() {
            struct_ser.serialize_field("keyPackageTlsSerialized", pbjson::private::base64::encode(&self.key_package_tls_serialized).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for KeyPackageUpload {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "key_package_tls_serialized",
            "keyPackageTlsSerialized",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            KeyPackageTlsSerialized,
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
                            "keyPackageTlsSerialized" | "key_package_tls_serialized" => Ok(GeneratedField::KeyPackageTlsSerialized),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = KeyPackageUpload;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.message_api.v3.KeyPackageUpload")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<KeyPackageUpload, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut key_package_tls_serialized__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::KeyPackageTlsSerialized => {
                            if key_package_tls_serialized__.is_some() {
                                return Err(serde::de::Error::duplicate_field("keyPackageTlsSerialized"));
                            }
                            key_package_tls_serialized__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(KeyPackageUpload {
                    key_package_tls_serialized: key_package_tls_serialized__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.message_api.v3.KeyPackageUpload", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for PublishToGroupRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.messages.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.message_api.v3.PublishToGroupRequest", len)?;
        if !self.messages.is_empty() {
            struct_ser.serialize_field("messages", &self.messages)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for PublishToGroupRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "messages",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Messages,
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
                            "messages" => Ok(GeneratedField::Messages),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = PublishToGroupRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.message_api.v3.PublishToGroupRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<PublishToGroupRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut messages__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Messages => {
                            if messages__.is_some() {
                                return Err(serde::de::Error::duplicate_field("messages"));
                            }
                            messages__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(PublishToGroupRequest {
                    messages: messages__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.message_api.v3.PublishToGroupRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for PublishWelcomesRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.welcome_messages.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.message_api.v3.PublishWelcomesRequest", len)?;
        if !self.welcome_messages.is_empty() {
            struct_ser.serialize_field("welcomeMessages", &self.welcome_messages)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for PublishWelcomesRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "welcome_messages",
            "welcomeMessages",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            WelcomeMessages,
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
                            "welcomeMessages" | "welcome_messages" => Ok(GeneratedField::WelcomeMessages),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = PublishWelcomesRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.message_api.v3.PublishWelcomesRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<PublishWelcomesRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut welcome_messages__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::WelcomeMessages => {
                            if welcome_messages__.is_some() {
                                return Err(serde::de::Error::duplicate_field("welcomeMessages"));
                            }
                            welcome_messages__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(PublishWelcomesRequest {
                    welcome_messages: welcome_messages__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.message_api.v3.PublishWelcomesRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for publish_welcomes_request::WelcomeMessageRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.installation_id.is_empty() {
            len += 1;
        }
        if self.welcome_message.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.message_api.v3.PublishWelcomesRequest.WelcomeMessageRequest", len)?;
        if !self.installation_id.is_empty() {
            struct_ser.serialize_field("installationId", pbjson::private::base64::encode(&self.installation_id).as_str())?;
        }
        if let Some(v) = self.welcome_message.as_ref() {
            struct_ser.serialize_field("welcomeMessage", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for publish_welcomes_request::WelcomeMessageRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "installation_id",
            "installationId",
            "welcome_message",
            "welcomeMessage",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InstallationId,
            WelcomeMessage,
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
                            "installationId" | "installation_id" => Ok(GeneratedField::InstallationId),
                            "welcomeMessage" | "welcome_message" => Ok(GeneratedField::WelcomeMessage),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = publish_welcomes_request::WelcomeMessageRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.message_api.v3.PublishWelcomesRequest.WelcomeMessageRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<publish_welcomes_request::WelcomeMessageRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut installation_id__ = None;
                let mut welcome_message__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::InstallationId => {
                            if installation_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationId"));
                            }
                            installation_id__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::WelcomeMessage => {
                            if welcome_message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("welcomeMessage"));
                            }
                            welcome_message__ = map.next_value()?;
                        }
                    }
                }
                Ok(publish_welcomes_request::WelcomeMessageRequest {
                    installation_id: installation_id__.unwrap_or_default(),
                    welcome_message: welcome_message__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.message_api.v3.PublishWelcomesRequest.WelcomeMessageRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RegisterInstallationRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.key_package.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.message_api.v3.RegisterInstallationRequest", len)?;
        if let Some(v) = self.key_package.as_ref() {
            struct_ser.serialize_field("keyPackage", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RegisterInstallationRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "key_package",
            "keyPackage",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            KeyPackage,
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
                            "keyPackage" | "key_package" => Ok(GeneratedField::KeyPackage),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = RegisterInstallationRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.message_api.v3.RegisterInstallationRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<RegisterInstallationRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut key_package__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::KeyPackage => {
                            if key_package__.is_some() {
                                return Err(serde::de::Error::duplicate_field("keyPackage"));
                            }
                            key_package__ = map.next_value()?;
                        }
                    }
                }
                Ok(RegisterInstallationRequest {
                    key_package: key_package__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.message_api.v3.RegisterInstallationRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RegisterInstallationResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.installation_id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.message_api.v3.RegisterInstallationResponse", len)?;
        if !self.installation_id.is_empty() {
            struct_ser.serialize_field("installationId", pbjson::private::base64::encode(&self.installation_id).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RegisterInstallationResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "installation_id",
            "installationId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InstallationId,
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
                            "installationId" | "installation_id" => Ok(GeneratedField::InstallationId),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = RegisterInstallationResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.message_api.v3.RegisterInstallationResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<RegisterInstallationResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut installation_id__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::InstallationId => {
                            if installation_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationId"));
                            }
                            installation_id__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(RegisterInstallationResponse {
                    installation_id: installation_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.message_api.v3.RegisterInstallationResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RevokeInstallationRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.installation_id.is_empty() {
            len += 1;
        }
        if self.wallet_signature.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.message_api.v3.RevokeInstallationRequest", len)?;
        if !self.installation_id.is_empty() {
            struct_ser.serialize_field("installationId", pbjson::private::base64::encode(&self.installation_id).as_str())?;
        }
        if let Some(v) = self.wallet_signature.as_ref() {
            struct_ser.serialize_field("walletSignature", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RevokeInstallationRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "installation_id",
            "installationId",
            "wallet_signature",
            "walletSignature",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InstallationId,
            WalletSignature,
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
                            "installationId" | "installation_id" => Ok(GeneratedField::InstallationId),
                            "walletSignature" | "wallet_signature" => Ok(GeneratedField::WalletSignature),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = RevokeInstallationRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.message_api.v3.RevokeInstallationRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<RevokeInstallationRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut installation_id__ = None;
                let mut wallet_signature__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::InstallationId => {
                            if installation_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationId"));
                            }
                            installation_id__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::WalletSignature => {
                            if wallet_signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("walletSignature"));
                            }
                            wallet_signature__ = map.next_value()?;
                        }
                    }
                }
                Ok(RevokeInstallationRequest {
                    installation_id: installation_id__.unwrap_or_default(),
                    wallet_signature: wallet_signature__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.message_api.v3.RevokeInstallationRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for UploadKeyPackageRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.key_package.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.message_api.v3.UploadKeyPackageRequest", len)?;
        if let Some(v) = self.key_package.as_ref() {
            struct_ser.serialize_field("keyPackage", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for UploadKeyPackageRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "key_package",
            "keyPackage",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            KeyPackage,
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
                            "keyPackage" | "key_package" => Ok(GeneratedField::KeyPackage),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = UploadKeyPackageRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.message_api.v3.UploadKeyPackageRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<UploadKeyPackageRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut key_package__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::KeyPackage => {
                            if key_package__.is_some() {
                                return Err(serde::de::Error::duplicate_field("keyPackage"));
                            }
                            key_package__ = map.next_value()?;
                        }
                    }
                }
                Ok(UploadKeyPackageRequest {
                    key_package: key_package__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.message_api.v3.UploadKeyPackageRequest", FIELDS, GeneratedVisitor)
    }
}
