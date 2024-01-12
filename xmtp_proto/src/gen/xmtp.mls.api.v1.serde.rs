// @generated
impl serde::Serialize for FetchKeyPackagesRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.installation_keys.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.FetchKeyPackagesRequest", len)?;
        if !self.installation_keys.is_empty() {
            struct_ser.serialize_field("installationKeys", &self.installation_keys.iter().map(pbjson::private::base64::encode).collect::<Vec<_>>())?;
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
            "installation_keys",
            "installationKeys",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InstallationKeys,
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
                            "installationKeys" | "installation_keys" => Ok(GeneratedField::InstallationKeys),
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
                formatter.write_str("struct xmtp.mls.api.v1.FetchKeyPackagesRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<FetchKeyPackagesRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut installation_keys__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::InstallationKeys => {
                            if installation_keys__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationKeys"));
                            }
                            installation_keys__ = 
                                Some(map.next_value::<Vec<::pbjson::private::BytesDeserialize<_>>>()?
                                    .into_iter().map(|x| x.0).collect())
                            ;
                        }
                    }
                }
                Ok(FetchKeyPackagesRequest {
                    installation_keys: installation_keys__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.api.v1.FetchKeyPackagesRequest", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.FetchKeyPackagesResponse", len)?;
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
                formatter.write_str("struct xmtp.mls.api.v1.FetchKeyPackagesResponse")
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
        deserializer.deserialize_struct("xmtp.mls.api.v1.FetchKeyPackagesResponse", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.FetchKeyPackagesResponse.KeyPackage", len)?;
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
                formatter.write_str("struct xmtp.mls.api.v1.FetchKeyPackagesResponse.KeyPackage")
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
        deserializer.deserialize_struct("xmtp.mls.api.v1.FetchKeyPackagesResponse.KeyPackage", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.GetIdentityUpdatesRequest", len)?;
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
                formatter.write_str("struct xmtp.mls.api.v1.GetIdentityUpdatesRequest")
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
        deserializer.deserialize_struct("xmtp.mls.api.v1.GetIdentityUpdatesRequest", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.GetIdentityUpdatesResponse", len)?;
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
                formatter.write_str("struct xmtp.mls.api.v1.GetIdentityUpdatesResponse")
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
        deserializer.deserialize_struct("xmtp.mls.api.v1.GetIdentityUpdatesResponse", FIELDS, GeneratedVisitor)
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
        if !self.installation_key.is_empty() {
            len += 1;
        }
        if !self.credential_identity.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.GetIdentityUpdatesResponse.NewInstallationUpdate", len)?;
        if !self.installation_key.is_empty() {
            struct_ser.serialize_field("installationKey", pbjson::private::base64::encode(&self.installation_key).as_str())?;
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
            "installation_key",
            "installationKey",
            "credential_identity",
            "credentialIdentity",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InstallationKey,
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
                            "installationKey" | "installation_key" => Ok(GeneratedField::InstallationKey),
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
                formatter.write_str("struct xmtp.mls.api.v1.GetIdentityUpdatesResponse.NewInstallationUpdate")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<get_identity_updates_response::NewInstallationUpdate, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut installation_key__ = None;
                let mut credential_identity__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::InstallationKey => {
                            if installation_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationKey"));
                            }
                            installation_key__ = 
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
                    installation_key: installation_key__.unwrap_or_default(),
                    credential_identity: credential_identity__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.api.v1.GetIdentityUpdatesResponse.NewInstallationUpdate", FIELDS, GeneratedVisitor)
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
        if !self.installation_key.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.GetIdentityUpdatesResponse.RevokedInstallationUpdate", len)?;
        if !self.installation_key.is_empty() {
            struct_ser.serialize_field("installationKey", pbjson::private::base64::encode(&self.installation_key).as_str())?;
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
            "installation_key",
            "installationKey",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InstallationKey,
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
                formatter.write_str("struct xmtp.mls.api.v1.GetIdentityUpdatesResponse.RevokedInstallationUpdate")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<get_identity_updates_response::RevokedInstallationUpdate, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut installation_key__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::InstallationKey => {
                            if installation_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationKey"));
                            }
                            installation_key__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(get_identity_updates_response::RevokedInstallationUpdate {
                    installation_key: installation_key__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.api.v1.GetIdentityUpdatesResponse.RevokedInstallationUpdate", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.GetIdentityUpdatesResponse.Update", len)?;
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
                formatter.write_str("struct xmtp.mls.api.v1.GetIdentityUpdatesResponse.Update")
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
        deserializer.deserialize_struct("xmtp.mls.api.v1.GetIdentityUpdatesResponse.Update", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.GetIdentityUpdatesResponse.WalletUpdates", len)?;
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
                formatter.write_str("struct xmtp.mls.api.v1.GetIdentityUpdatesResponse.WalletUpdates")
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
        deserializer.deserialize_struct("xmtp.mls.api.v1.GetIdentityUpdatesResponse.WalletUpdates", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.GroupMessage", len)?;
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
                formatter.write_str("struct xmtp.mls.api.v1.GroupMessage")
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
        deserializer.deserialize_struct("xmtp.mls.api.v1.GroupMessage", FIELDS, GeneratedVisitor)
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
        if self.id != 0 {
            len += 1;
        }
        if self.created_ns != 0 {
            len += 1;
        }
        if !self.group_id.is_empty() {
            len += 1;
        }
        if !self.data.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.GroupMessage.V1", len)?;
        if self.id != 0 {
            struct_ser.serialize_field("id", ToString::to_string(&self.id).as_str())?;
        }
        if self.created_ns != 0 {
            struct_ser.serialize_field("createdNs", ToString::to_string(&self.created_ns).as_str())?;
        }
        if !self.group_id.is_empty() {
            struct_ser.serialize_field("groupId", pbjson::private::base64::encode(&self.group_id).as_str())?;
        }
        if !self.data.is_empty() {
            struct_ser.serialize_field("data", pbjson::private::base64::encode(&self.data).as_str())?;
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
            "id",
            "created_ns",
            "createdNs",
            "group_id",
            "groupId",
            "data",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Id,
            CreatedNs,
            GroupId,
            Data,
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
                            "createdNs" | "created_ns" => Ok(GeneratedField::CreatedNs),
                            "groupId" | "group_id" => Ok(GeneratedField::GroupId),
                            "data" => Ok(GeneratedField::Data),
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
                formatter.write_str("struct xmtp.mls.api.v1.GroupMessage.V1")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<group_message::V1, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut id__ = None;
                let mut created_ns__ = None;
                let mut group_id__ = None;
                let mut data__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Id => {
                            if id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("id"));
                            }
                            id__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::CreatedNs => {
                            if created_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createdNs"));
                            }
                            created_ns__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::GroupId => {
                            if group_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("groupId"));
                            }
                            group_id__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Data => {
                            if data__.is_some() {
                                return Err(serde::de::Error::duplicate_field("data"));
                            }
                            data__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(group_message::V1 {
                    id: id__.unwrap_or_default(),
                    created_ns: created_ns__.unwrap_or_default(),
                    group_id: group_id__.unwrap_or_default(),
                    data: data__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.api.v1.GroupMessage.V1", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GroupMessageInput {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.GroupMessageInput", len)?;
        if let Some(v) = self.version.as_ref() {
            match v {
                group_message_input::Version::V1(v) => {
                    struct_ser.serialize_field("v1", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GroupMessageInput {
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
            type Value = GroupMessageInput;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.api.v1.GroupMessageInput")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<GroupMessageInput, V::Error>
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
                            version__ = map.next_value::<::std::option::Option<_>>()?.map(group_message_input::Version::V1)
;
                        }
                    }
                }
                Ok(GroupMessageInput {
                    version: version__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.api.v1.GroupMessageInput", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for group_message_input::V1 {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.data.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.GroupMessageInput.V1", len)?;
        if !self.data.is_empty() {
            struct_ser.serialize_field("data", pbjson::private::base64::encode(&self.data).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for group_message_input::V1 {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "data",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Data,
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
                            "data" => Ok(GeneratedField::Data),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = group_message_input::V1;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.api.v1.GroupMessageInput.V1")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<group_message_input::V1, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut data__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Data => {
                            if data__.is_some() {
                                return Err(serde::de::Error::duplicate_field("data"));
                            }
                            data__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(group_message_input::V1 {
                    data: data__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.api.v1.GroupMessageInput.V1", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.KeyPackageUpload", len)?;
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
                formatter.write_str("struct xmtp.mls.api.v1.KeyPackageUpload")
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
        deserializer.deserialize_struct("xmtp.mls.api.v1.KeyPackageUpload", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for PagingInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.direction != 0 {
            len += 1;
        }
        if self.limit != 0 {
            len += 1;
        }
        if self.id_cursor != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.PagingInfo", len)?;
        if self.direction != 0 {
            let v = SortDirection::from_i32(self.direction)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.direction)))?;
            struct_ser.serialize_field("direction", &v)?;
        }
        if self.limit != 0 {
            struct_ser.serialize_field("limit", &self.limit)?;
        }
        if self.id_cursor != 0 {
            struct_ser.serialize_field("idCursor", ToString::to_string(&self.id_cursor).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for PagingInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "direction",
            "limit",
            "id_cursor",
            "idCursor",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Direction,
            Limit,
            IdCursor,
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
                            "direction" => Ok(GeneratedField::Direction),
                            "limit" => Ok(GeneratedField::Limit),
                            "idCursor" | "id_cursor" => Ok(GeneratedField::IdCursor),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = PagingInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.api.v1.PagingInfo")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<PagingInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut direction__ = None;
                let mut limit__ = None;
                let mut id_cursor__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Direction => {
                            if direction__.is_some() {
                                return Err(serde::de::Error::duplicate_field("direction"));
                            }
                            direction__ = Some(map.next_value::<SortDirection>()? as i32);
                        }
                        GeneratedField::Limit => {
                            if limit__.is_some() {
                                return Err(serde::de::Error::duplicate_field("limit"));
                            }
                            limit__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::IdCursor => {
                            if id_cursor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("idCursor"));
                            }
                            id_cursor__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(PagingInfo {
                    direction: direction__.unwrap_or_default(),
                    limit: limit__.unwrap_or_default(),
                    id_cursor: id_cursor__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.api.v1.PagingInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for QueryGroupMessagesRequest {
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
        if self.paging_info.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.QueryGroupMessagesRequest", len)?;
        if !self.group_id.is_empty() {
            struct_ser.serialize_field("groupId", pbjson::private::base64::encode(&self.group_id).as_str())?;
        }
        if let Some(v) = self.paging_info.as_ref() {
            struct_ser.serialize_field("pagingInfo", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for QueryGroupMessagesRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "group_id",
            "groupId",
            "paging_info",
            "pagingInfo",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            GroupId,
            PagingInfo,
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
                            "pagingInfo" | "paging_info" => Ok(GeneratedField::PagingInfo),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = QueryGroupMessagesRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.api.v1.QueryGroupMessagesRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<QueryGroupMessagesRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut group_id__ = None;
                let mut paging_info__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::GroupId => {
                            if group_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("groupId"));
                            }
                            group_id__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::PagingInfo => {
                            if paging_info__.is_some() {
                                return Err(serde::de::Error::duplicate_field("pagingInfo"));
                            }
                            paging_info__ = map.next_value()?;
                        }
                    }
                }
                Ok(QueryGroupMessagesRequest {
                    group_id: group_id__.unwrap_or_default(),
                    paging_info: paging_info__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.api.v1.QueryGroupMessagesRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for QueryGroupMessagesResponse {
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
        if self.paging_info.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.QueryGroupMessagesResponse", len)?;
        if !self.messages.is_empty() {
            struct_ser.serialize_field("messages", &self.messages)?;
        }
        if let Some(v) = self.paging_info.as_ref() {
            struct_ser.serialize_field("pagingInfo", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for QueryGroupMessagesResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "messages",
            "paging_info",
            "pagingInfo",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Messages,
            PagingInfo,
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
                            "pagingInfo" | "paging_info" => Ok(GeneratedField::PagingInfo),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = QueryGroupMessagesResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.api.v1.QueryGroupMessagesResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<QueryGroupMessagesResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut messages__ = None;
                let mut paging_info__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Messages => {
                            if messages__.is_some() {
                                return Err(serde::de::Error::duplicate_field("messages"));
                            }
                            messages__ = Some(map.next_value()?);
                        }
                        GeneratedField::PagingInfo => {
                            if paging_info__.is_some() {
                                return Err(serde::de::Error::duplicate_field("pagingInfo"));
                            }
                            paging_info__ = map.next_value()?;
                        }
                    }
                }
                Ok(QueryGroupMessagesResponse {
                    messages: messages__.unwrap_or_default(),
                    paging_info: paging_info__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.api.v1.QueryGroupMessagesResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for QueryWelcomeMessagesRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.installation_key.is_empty() {
            len += 1;
        }
        if self.paging_info.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.QueryWelcomeMessagesRequest", len)?;
        if !self.installation_key.is_empty() {
            struct_ser.serialize_field("installationKey", pbjson::private::base64::encode(&self.installation_key).as_str())?;
        }
        if let Some(v) = self.paging_info.as_ref() {
            struct_ser.serialize_field("pagingInfo", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for QueryWelcomeMessagesRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "installation_key",
            "installationKey",
            "paging_info",
            "pagingInfo",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InstallationKey,
            PagingInfo,
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
                            "pagingInfo" | "paging_info" => Ok(GeneratedField::PagingInfo),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = QueryWelcomeMessagesRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.api.v1.QueryWelcomeMessagesRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<QueryWelcomeMessagesRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut installation_key__ = None;
                let mut paging_info__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::InstallationKey => {
                            if installation_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationKey"));
                            }
                            installation_key__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::PagingInfo => {
                            if paging_info__.is_some() {
                                return Err(serde::de::Error::duplicate_field("pagingInfo"));
                            }
                            paging_info__ = map.next_value()?;
                        }
                    }
                }
                Ok(QueryWelcomeMessagesRequest {
                    installation_key: installation_key__.unwrap_or_default(),
                    paging_info: paging_info__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.api.v1.QueryWelcomeMessagesRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for QueryWelcomeMessagesResponse {
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
        if self.paging_info.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.QueryWelcomeMessagesResponse", len)?;
        if !self.messages.is_empty() {
            struct_ser.serialize_field("messages", &self.messages)?;
        }
        if let Some(v) = self.paging_info.as_ref() {
            struct_ser.serialize_field("pagingInfo", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for QueryWelcomeMessagesResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "messages",
            "paging_info",
            "pagingInfo",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Messages,
            PagingInfo,
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
                            "pagingInfo" | "paging_info" => Ok(GeneratedField::PagingInfo),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = QueryWelcomeMessagesResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.api.v1.QueryWelcomeMessagesResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<QueryWelcomeMessagesResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut messages__ = None;
                let mut paging_info__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Messages => {
                            if messages__.is_some() {
                                return Err(serde::de::Error::duplicate_field("messages"));
                            }
                            messages__ = Some(map.next_value()?);
                        }
                        GeneratedField::PagingInfo => {
                            if paging_info__.is_some() {
                                return Err(serde::de::Error::duplicate_field("pagingInfo"));
                            }
                            paging_info__ = map.next_value()?;
                        }
                    }
                }
                Ok(QueryWelcomeMessagesResponse {
                    messages: messages__.unwrap_or_default(),
                    paging_info: paging_info__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.api.v1.QueryWelcomeMessagesResponse", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.RegisterInstallationRequest", len)?;
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
                formatter.write_str("struct xmtp.mls.api.v1.RegisterInstallationRequest")
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
        deserializer.deserialize_struct("xmtp.mls.api.v1.RegisterInstallationRequest", FIELDS, GeneratedVisitor)
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
        if !self.installation_key.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.RegisterInstallationResponse", len)?;
        if !self.installation_key.is_empty() {
            struct_ser.serialize_field("installationKey", pbjson::private::base64::encode(&self.installation_key).as_str())?;
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
            "installation_key",
            "installationKey",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InstallationKey,
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
                formatter.write_str("struct xmtp.mls.api.v1.RegisterInstallationResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<RegisterInstallationResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut installation_key__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::InstallationKey => {
                            if installation_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationKey"));
                            }
                            installation_key__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(RegisterInstallationResponse {
                    installation_key: installation_key__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.api.v1.RegisterInstallationResponse", FIELDS, GeneratedVisitor)
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
        if !self.installation_key.is_empty() {
            len += 1;
        }
        if self.wallet_signature.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.RevokeInstallationRequest", len)?;
        if !self.installation_key.is_empty() {
            struct_ser.serialize_field("installationKey", pbjson::private::base64::encode(&self.installation_key).as_str())?;
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
            "installation_key",
            "installationKey",
            "wallet_signature",
            "walletSignature",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InstallationKey,
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
                            "installationKey" | "installation_key" => Ok(GeneratedField::InstallationKey),
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
                formatter.write_str("struct xmtp.mls.api.v1.RevokeInstallationRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<RevokeInstallationRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut installation_key__ = None;
                let mut wallet_signature__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::InstallationKey => {
                            if installation_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationKey"));
                            }
                            installation_key__ = 
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
                    installation_key: installation_key__.unwrap_or_default(),
                    wallet_signature: wallet_signature__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.api.v1.RevokeInstallationRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SendGroupMessagesRequest {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.SendGroupMessagesRequest", len)?;
        if !self.messages.is_empty() {
            struct_ser.serialize_field("messages", &self.messages)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SendGroupMessagesRequest {
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
            type Value = SendGroupMessagesRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.api.v1.SendGroupMessagesRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SendGroupMessagesRequest, V::Error>
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
                Ok(SendGroupMessagesRequest {
                    messages: messages__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.api.v1.SendGroupMessagesRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SendWelcomeMessagesRequest {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.SendWelcomeMessagesRequest", len)?;
        if !self.messages.is_empty() {
            struct_ser.serialize_field("messages", &self.messages)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SendWelcomeMessagesRequest {
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
            type Value = SendWelcomeMessagesRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.api.v1.SendWelcomeMessagesRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SendWelcomeMessagesRequest, V::Error>
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
                Ok(SendWelcomeMessagesRequest {
                    messages: messages__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.api.v1.SendWelcomeMessagesRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SortDirection {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "SORT_DIRECTION_UNSPECIFIED",
            Self::Ascending => "SORT_DIRECTION_ASCENDING",
            Self::Descending => "SORT_DIRECTION_DESCENDING",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for SortDirection {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "SORT_DIRECTION_UNSPECIFIED",
            "SORT_DIRECTION_ASCENDING",
            "SORT_DIRECTION_DESCENDING",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SortDirection;

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
                    .and_then(SortDirection::from_i32)
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
                    .and_then(SortDirection::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "SORT_DIRECTION_UNSPECIFIED" => Ok(SortDirection::Unspecified),
                    "SORT_DIRECTION_ASCENDING" => Ok(SortDirection::Ascending),
                    "SORT_DIRECTION_DESCENDING" => Ok(SortDirection::Descending),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for SubscribeFilterType {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "SUBSCRIBE_FILTER_TYPE_UNSPECIFIED",
            Self::Latest => "SUBSCRIBE_FILTER_TYPE_LATEST",
            Self::Cursor => "SUBSCRIBE_FILTER_TYPE_CURSOR",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for SubscribeFilterType {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "SUBSCRIBE_FILTER_TYPE_UNSPECIFIED",
            "SUBSCRIBE_FILTER_TYPE_LATEST",
            "SUBSCRIBE_FILTER_TYPE_CURSOR",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SubscribeFilterType;

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
                    .and_then(SubscribeFilterType::from_i32)
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
                    .and_then(SubscribeFilterType::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "SUBSCRIBE_FILTER_TYPE_UNSPECIFIED" => Ok(SubscribeFilterType::Unspecified),
                    "SUBSCRIBE_FILTER_TYPE_LATEST" => Ok(SubscribeFilterType::Latest),
                    "SUBSCRIBE_FILTER_TYPE_CURSOR" => Ok(SubscribeFilterType::Cursor),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for SubscribeGroupMessagesRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.filters.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.SubscribeGroupMessagesRequest", len)?;
        if !self.filters.is_empty() {
            struct_ser.serialize_field("filters", &self.filters)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SubscribeGroupMessagesRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "filters",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Filters,
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
                            "filters" => Ok(GeneratedField::Filters),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SubscribeGroupMessagesRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.api.v1.SubscribeGroupMessagesRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SubscribeGroupMessagesRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut filters__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Filters => {
                            if filters__.is_some() {
                                return Err(serde::de::Error::duplicate_field("filters"));
                            }
                            filters__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(SubscribeGroupMessagesRequest {
                    filters: filters__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.api.v1.SubscribeGroupMessagesRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for subscribe_group_messages_request::Filter {
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
        if self.r#type != 0 {
            len += 1;
        }
        if self.id_cursor != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.SubscribeGroupMessagesRequest.Filter", len)?;
        if !self.group_id.is_empty() {
            struct_ser.serialize_field("groupId", pbjson::private::base64::encode(&self.group_id).as_str())?;
        }
        if self.r#type != 0 {
            let v = SubscribeFilterType::from_i32(self.r#type)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.r#type)))?;
            struct_ser.serialize_field("type", &v)?;
        }
        if self.id_cursor != 0 {
            struct_ser.serialize_field("idCursor", ToString::to_string(&self.id_cursor).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for subscribe_group_messages_request::Filter {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "group_id",
            "groupId",
            "type",
            "id_cursor",
            "idCursor",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            GroupId,
            Type,
            IdCursor,
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
                            "type" => Ok(GeneratedField::Type),
                            "idCursor" | "id_cursor" => Ok(GeneratedField::IdCursor),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = subscribe_group_messages_request::Filter;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.api.v1.SubscribeGroupMessagesRequest.Filter")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<subscribe_group_messages_request::Filter, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut group_id__ = None;
                let mut r#type__ = None;
                let mut id_cursor__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::GroupId => {
                            if group_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("groupId"));
                            }
                            group_id__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Type => {
                            if r#type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("type"));
                            }
                            r#type__ = Some(map.next_value::<SubscribeFilterType>()? as i32);
                        }
                        GeneratedField::IdCursor => {
                            if id_cursor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("idCursor"));
                            }
                            id_cursor__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(subscribe_group_messages_request::Filter {
                    group_id: group_id__.unwrap_or_default(),
                    r#type: r#type__.unwrap_or_default(),
                    id_cursor: id_cursor__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.api.v1.SubscribeGroupMessagesRequest.Filter", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SubscribeWelcomeMessagesRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.filters.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.SubscribeWelcomeMessagesRequest", len)?;
        if !self.filters.is_empty() {
            struct_ser.serialize_field("filters", &self.filters)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SubscribeWelcomeMessagesRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "filters",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Filters,
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
                            "filters" => Ok(GeneratedField::Filters),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SubscribeWelcomeMessagesRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.api.v1.SubscribeWelcomeMessagesRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SubscribeWelcomeMessagesRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut filters__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Filters => {
                            if filters__.is_some() {
                                return Err(serde::de::Error::duplicate_field("filters"));
                            }
                            filters__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(SubscribeWelcomeMessagesRequest {
                    filters: filters__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.api.v1.SubscribeWelcomeMessagesRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for subscribe_welcome_messages_request::Filter {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.installation_key.is_empty() {
            len += 1;
        }
        if self.r#type != 0 {
            len += 1;
        }
        if self.id_cursor != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.SubscribeWelcomeMessagesRequest.Filter", len)?;
        if !self.installation_key.is_empty() {
            struct_ser.serialize_field("installationKey", pbjson::private::base64::encode(&self.installation_key).as_str())?;
        }
        if self.r#type != 0 {
            let v = SubscribeFilterType::from_i32(self.r#type)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.r#type)))?;
            struct_ser.serialize_field("type", &v)?;
        }
        if self.id_cursor != 0 {
            struct_ser.serialize_field("idCursor", ToString::to_string(&self.id_cursor).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for subscribe_welcome_messages_request::Filter {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "installation_key",
            "installationKey",
            "type",
            "id_cursor",
            "idCursor",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InstallationKey,
            Type,
            IdCursor,
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
                            "type" => Ok(GeneratedField::Type),
                            "idCursor" | "id_cursor" => Ok(GeneratedField::IdCursor),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = subscribe_welcome_messages_request::Filter;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.api.v1.SubscribeWelcomeMessagesRequest.Filter")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<subscribe_welcome_messages_request::Filter, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut installation_key__ = None;
                let mut r#type__ = None;
                let mut id_cursor__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::InstallationKey => {
                            if installation_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationKey"));
                            }
                            installation_key__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Type => {
                            if r#type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("type"));
                            }
                            r#type__ = Some(map.next_value::<SubscribeFilterType>()? as i32);
                        }
                        GeneratedField::IdCursor => {
                            if id_cursor__.is_some() {
                                return Err(serde::de::Error::duplicate_field("idCursor"));
                            }
                            id_cursor__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(subscribe_welcome_messages_request::Filter {
                    installation_key: installation_key__.unwrap_or_default(),
                    r#type: r#type__.unwrap_or_default(),
                    id_cursor: id_cursor__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.api.v1.SubscribeWelcomeMessagesRequest.Filter", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.UploadKeyPackageRequest", len)?;
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
                formatter.write_str("struct xmtp.mls.api.v1.UploadKeyPackageRequest")
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
        deserializer.deserialize_struct("xmtp.mls.api.v1.UploadKeyPackageRequest", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.WelcomeMessage", len)?;
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
                formatter.write_str("struct xmtp.mls.api.v1.WelcomeMessage")
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
        deserializer.deserialize_struct("xmtp.mls.api.v1.WelcomeMessage", FIELDS, GeneratedVisitor)
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
        if self.id != 0 {
            len += 1;
        }
        if self.created_ns != 0 {
            len += 1;
        }
        if !self.installation_key.is_empty() {
            len += 1;
        }
        if !self.data.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.WelcomeMessage.V1", len)?;
        if self.id != 0 {
            struct_ser.serialize_field("id", ToString::to_string(&self.id).as_str())?;
        }
        if self.created_ns != 0 {
            struct_ser.serialize_field("createdNs", ToString::to_string(&self.created_ns).as_str())?;
        }
        if !self.installation_key.is_empty() {
            struct_ser.serialize_field("installationKey", pbjson::private::base64::encode(&self.installation_key).as_str())?;
        }
        if !self.data.is_empty() {
            struct_ser.serialize_field("data", pbjson::private::base64::encode(&self.data).as_str())?;
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
            "id",
            "created_ns",
            "createdNs",
            "installation_key",
            "installationKey",
            "data",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Id,
            CreatedNs,
            InstallationKey,
            Data,
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
                            "createdNs" | "created_ns" => Ok(GeneratedField::CreatedNs),
                            "installationKey" | "installation_key" => Ok(GeneratedField::InstallationKey),
                            "data" => Ok(GeneratedField::Data),
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
                formatter.write_str("struct xmtp.mls.api.v1.WelcomeMessage.V1")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<welcome_message::V1, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut id__ = None;
                let mut created_ns__ = None;
                let mut installation_key__ = None;
                let mut data__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Id => {
                            if id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("id"));
                            }
                            id__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::CreatedNs => {
                            if created_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createdNs"));
                            }
                            created_ns__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::InstallationKey => {
                            if installation_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationKey"));
                            }
                            installation_key__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Data => {
                            if data__.is_some() {
                                return Err(serde::de::Error::duplicate_field("data"));
                            }
                            data__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(welcome_message::V1 {
                    id: id__.unwrap_or_default(),
                    created_ns: created_ns__.unwrap_or_default(),
                    installation_key: installation_key__.unwrap_or_default(),
                    data: data__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.api.v1.WelcomeMessage.V1", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for WelcomeMessageInput {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.WelcomeMessageInput", len)?;
        if let Some(v) = self.version.as_ref() {
            match v {
                welcome_message_input::Version::V1(v) => {
                    struct_ser.serialize_field("v1", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for WelcomeMessageInput {
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
            type Value = WelcomeMessageInput;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.api.v1.WelcomeMessageInput")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<WelcomeMessageInput, V::Error>
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
                            version__ = map.next_value::<::std::option::Option<_>>()?.map(welcome_message_input::Version::V1)
;
                        }
                    }
                }
                Ok(WelcomeMessageInput {
                    version: version__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.api.v1.WelcomeMessageInput", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for welcome_message_input::V1 {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.installation_key.is_empty() {
            len += 1;
        }
        if !self.data.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.api.v1.WelcomeMessageInput.V1", len)?;
        if !self.installation_key.is_empty() {
            struct_ser.serialize_field("installationKey", pbjson::private::base64::encode(&self.installation_key).as_str())?;
        }
        if !self.data.is_empty() {
            struct_ser.serialize_field("data", pbjson::private::base64::encode(&self.data).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for welcome_message_input::V1 {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "installation_key",
            "installationKey",
            "data",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InstallationKey,
            Data,
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
                            "data" => Ok(GeneratedField::Data),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = welcome_message_input::V1;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.api.v1.WelcomeMessageInput.V1")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<welcome_message_input::V1, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut installation_key__ = None;
                let mut data__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::InstallationKey => {
                            if installation_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationKey"));
                            }
                            installation_key__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Data => {
                            if data__.is_some() {
                                return Err(serde::de::Error::duplicate_field("data"));
                            }
                            data__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(welcome_message_input::V1 {
                    installation_key: installation_key__.unwrap_or_default(),
                    data: data__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.api.v1.WelcomeMessageInput.V1", FIELDS, GeneratedVisitor)
    }
}
