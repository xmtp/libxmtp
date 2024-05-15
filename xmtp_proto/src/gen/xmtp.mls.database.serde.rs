// @generated
impl serde::Serialize for AccountAddresses {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.AccountAddresses", len)?;
        if !self.account_addresses.is_empty() {
            struct_ser.serialize_field("accountAddresses", &self.account_addresses)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AccountAddresses {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "account_addresses",
            "accountAddresses",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AccountAddresses,
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
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AccountAddresses;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.AccountAddresses")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AccountAddresses, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut account_addresses__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AccountAddresses => {
                            if account_addresses__.is_some() {
                                return Err(serde::de::Error::duplicate_field("accountAddresses"));
                            }
                            account_addresses__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(AccountAddresses {
                    account_addresses: account_addresses__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.AccountAddresses", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AddMembersData {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.AddMembersData", len)?;
        if let Some(v) = self.version.as_ref() {
            match v {
                add_members_data::Version::V1(v) => {
                    struct_ser.serialize_field("v1", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AddMembersData {
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
            type Value = AddMembersData;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.AddMembersData")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AddMembersData, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut version__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::V1 => {
                            if version__.is_some() {
                                return Err(serde::de::Error::duplicate_field("v1"));
                            }
                            version__ = map_.next_value::<::std::option::Option<_>>()?.map(add_members_data::Version::V1)
;
                        }
                    }
                }
                Ok(AddMembersData {
                    version: version__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.AddMembersData", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for add_members_data::V1 {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.addresses_or_installation_ids.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.AddMembersData.V1", len)?;
        if let Some(v) = self.addresses_or_installation_ids.as_ref() {
            struct_ser.serialize_field("addressesOrInstallationIds", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for add_members_data::V1 {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "addresses_or_installation_ids",
            "addressesOrInstallationIds",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AddressesOrInstallationIds,
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
                            "addressesOrInstallationIds" | "addresses_or_installation_ids" => Ok(GeneratedField::AddressesOrInstallationIds),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = add_members_data::V1;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.AddMembersData.V1")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<add_members_data::V1, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut addresses_or_installation_ids__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AddressesOrInstallationIds => {
                            if addresses_or_installation_ids__.is_some() {
                                return Err(serde::de::Error::duplicate_field("addressesOrInstallationIds"));
                            }
                            addresses_or_installation_ids__ = map_.next_value()?;
                        }
                    }
                }
                Ok(add_members_data::V1 {
                    addresses_or_installation_ids: addresses_or_installation_ids__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.AddMembersData.V1", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AddressesOrInstallationIds {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.addresses_or_installation_ids.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.AddressesOrInstallationIds", len)?;
        if let Some(v) = self.addresses_or_installation_ids.as_ref() {
            match v {
                addresses_or_installation_ids::AddressesOrInstallationIds::AccountAddresses(v) => {
                    struct_ser.serialize_field("accountAddresses", v)?;
                }
                addresses_or_installation_ids::AddressesOrInstallationIds::InstallationIds(v) => {
                    struct_ser.serialize_field("installationIds", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AddressesOrInstallationIds {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "account_addresses",
            "accountAddresses",
            "installation_ids",
            "installationIds",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AccountAddresses,
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
                            "accountAddresses" | "account_addresses" => Ok(GeneratedField::AccountAddresses),
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
            type Value = AddressesOrInstallationIds;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.AddressesOrInstallationIds")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AddressesOrInstallationIds, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut addresses_or_installation_ids__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AccountAddresses => {
                            if addresses_or_installation_ids__.is_some() {
                                return Err(serde::de::Error::duplicate_field("accountAddresses"));
                            }
                            addresses_or_installation_ids__ = map_.next_value::<::std::option::Option<_>>()?.map(addresses_or_installation_ids::AddressesOrInstallationIds::AccountAddresses)
;
                        }
                        GeneratedField::InstallationIds => {
                            if addresses_or_installation_ids__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationIds"));
                            }
                            addresses_or_installation_ids__ = map_.next_value::<::std::option::Option<_>>()?.map(addresses_or_installation_ids::AddressesOrInstallationIds::InstallationIds)
;
                        }
                    }
                }
                Ok(AddressesOrInstallationIds {
                    addresses_or_installation_ids: addresses_or_installation_ids__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.AddressesOrInstallationIds", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AdminListUpdateType {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::AddAdmin => "ADD_ADMIN",
            Self::RemoveAdmin => "REMOVE_ADMIN",
            Self::AddSuperAdmin => "ADD_SUPER_ADMIN",
            Self::RemoveSuperAdmin => "REMOVE_SUPER_ADMIN",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for AdminListUpdateType {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "ADD_ADMIN",
            "REMOVE_ADMIN",
            "ADD_SUPER_ADMIN",
            "REMOVE_SUPER_ADMIN",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AdminListUpdateType;

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
                    "ADD_ADMIN" => Ok(AdminListUpdateType::AddAdmin),
                    "REMOVE_ADMIN" => Ok(AdminListUpdateType::RemoveAdmin),
                    "ADD_SUPER_ADMIN" => Ok(AdminListUpdateType::AddSuperAdmin),
                    "REMOVE_SUPER_ADMIN" => Ok(AdminListUpdateType::RemoveSuperAdmin),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for InstallationIds {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.InstallationIds", len)?;
        if !self.installation_ids.is_empty() {
            struct_ser.serialize_field("installationIds", &self.installation_ids.iter().map(pbjson::private::base64::encode).collect::<Vec<_>>())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for InstallationIds {
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
            type Value = InstallationIds;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.InstallationIds")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<InstallationIds, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut installation_ids__ = None;
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
                    }
                }
                Ok(InstallationIds {
                    installation_ids: installation_ids__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.InstallationIds", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for PostCommitAction {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.PostCommitAction", len)?;
        if let Some(v) = self.kind.as_ref() {
            match v {
                post_commit_action::Kind::SendWelcomes(v) => {
                    struct_ser.serialize_field("sendWelcomes", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for PostCommitAction {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "send_welcomes",
            "sendWelcomes",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            SendWelcomes,
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
                            "sendWelcomes" | "send_welcomes" => Ok(GeneratedField::SendWelcomes),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = PostCommitAction;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.PostCommitAction")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<PostCommitAction, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut kind__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::SendWelcomes => {
                            if kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sendWelcomes"));
                            }
                            kind__ = map_.next_value::<::std::option::Option<_>>()?.map(post_commit_action::Kind::SendWelcomes)
;
                        }
                    }
                }
                Ok(PostCommitAction {
                    kind: kind__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.PostCommitAction", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for post_commit_action::Installation {
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
        if !self.hpke_public_key.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.PostCommitAction.Installation", len)?;
        if !self.installation_key.is_empty() {
            #[allow(clippy::needless_borrow)]
            struct_ser.serialize_field("installationKey", pbjson::private::base64::encode(&self.installation_key).as_str())?;
        }
        if !self.hpke_public_key.is_empty() {
            #[allow(clippy::needless_borrow)]
            struct_ser.serialize_field("hpkePublicKey", pbjson::private::base64::encode(&self.hpke_public_key).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for post_commit_action::Installation {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "installation_key",
            "installationKey",
            "hpke_public_key",
            "hpkePublicKey",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InstallationKey,
            HpkePublicKey,
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
                            "hpkePublicKey" | "hpke_public_key" => Ok(GeneratedField::HpkePublicKey),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = post_commit_action::Installation;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.PostCommitAction.Installation")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<post_commit_action::Installation, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut installation_key__ = None;
                let mut hpke_public_key__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::InstallationKey => {
                            if installation_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationKey"));
                            }
                            installation_key__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::HpkePublicKey => {
                            if hpke_public_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("hpkePublicKey"));
                            }
                            hpke_public_key__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(post_commit_action::Installation {
                    installation_key: installation_key__.unwrap_or_default(),
                    hpke_public_key: hpke_public_key__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.PostCommitAction.Installation", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for post_commit_action::SendWelcomes {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.installations.is_empty() {
            len += 1;
        }
        if !self.welcome_message.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.PostCommitAction.SendWelcomes", len)?;
        if !self.installations.is_empty() {
            struct_ser.serialize_field("installations", &self.installations)?;
        }
        if !self.welcome_message.is_empty() {
            #[allow(clippy::needless_borrow)]
            struct_ser.serialize_field("welcomeMessage", pbjson::private::base64::encode(&self.welcome_message).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for post_commit_action::SendWelcomes {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "installations",
            "welcome_message",
            "welcomeMessage",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Installations,
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
                            "installations" => Ok(GeneratedField::Installations),
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
            type Value = post_commit_action::SendWelcomes;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.PostCommitAction.SendWelcomes")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<post_commit_action::SendWelcomes, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut installations__ = None;
                let mut welcome_message__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Installations => {
                            if installations__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installations"));
                            }
                            installations__ = Some(map_.next_value()?);
                        }
                        GeneratedField::WelcomeMessage => {
                            if welcome_message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("welcomeMessage"));
                            }
                            welcome_message__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(post_commit_action::SendWelcomes {
                    installations: installations__.unwrap_or_default(),
                    welcome_message: welcome_message__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.PostCommitAction.SendWelcomes", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RemoveMembersData {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.RemoveMembersData", len)?;
        if let Some(v) = self.version.as_ref() {
            match v {
                remove_members_data::Version::V1(v) => {
                    struct_ser.serialize_field("v1", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RemoveMembersData {
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
            type Value = RemoveMembersData;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.RemoveMembersData")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<RemoveMembersData, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut version__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::V1 => {
                            if version__.is_some() {
                                return Err(serde::de::Error::duplicate_field("v1"));
                            }
                            version__ = map_.next_value::<::std::option::Option<_>>()?.map(remove_members_data::Version::V1)
;
                        }
                    }
                }
                Ok(RemoveMembersData {
                    version: version__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.RemoveMembersData", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for remove_members_data::V1 {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.addresses_or_installation_ids.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.RemoveMembersData.V1", len)?;
        if let Some(v) = self.addresses_or_installation_ids.as_ref() {
            struct_ser.serialize_field("addressesOrInstallationIds", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for remove_members_data::V1 {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "addresses_or_installation_ids",
            "addressesOrInstallationIds",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AddressesOrInstallationIds,
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
                            "addressesOrInstallationIds" | "addresses_or_installation_ids" => Ok(GeneratedField::AddressesOrInstallationIds),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = remove_members_data::V1;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.RemoveMembersData.V1")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<remove_members_data::V1, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut addresses_or_installation_ids__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AddressesOrInstallationIds => {
                            if addresses_or_installation_ids__.is_some() {
                                return Err(serde::de::Error::duplicate_field("addressesOrInstallationIds"));
                            }
                            addresses_or_installation_ids__ = map_.next_value()?;
                        }
                    }
                }
                Ok(remove_members_data::V1 {
                    addresses_or_installation_ids: addresses_or_installation_ids__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.RemoveMembersData.V1", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SendMessageData {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.SendMessageData", len)?;
        if let Some(v) = self.version.as_ref() {
            match v {
                send_message_data::Version::V1(v) => {
                    struct_ser.serialize_field("v1", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SendMessageData {
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
            type Value = SendMessageData;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.SendMessageData")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SendMessageData, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut version__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::V1 => {
                            if version__.is_some() {
                                return Err(serde::de::Error::duplicate_field("v1"));
                            }
                            version__ = map_.next_value::<::std::option::Option<_>>()?.map(send_message_data::Version::V1)
;
                        }
                    }
                }
                Ok(SendMessageData {
                    version: version__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.SendMessageData", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for send_message_data::V1 {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.payload_bytes.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.SendMessageData.V1", len)?;
        if !self.payload_bytes.is_empty() {
            #[allow(clippy::needless_borrow)]
            struct_ser.serialize_field("payloadBytes", pbjson::private::base64::encode(&self.payload_bytes).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for send_message_data::V1 {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "payload_bytes",
            "payloadBytes",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            PayloadBytes,
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
                            "payloadBytes" | "payload_bytes" => Ok(GeneratedField::PayloadBytes),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = send_message_data::V1;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.SendMessageData.V1")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<send_message_data::V1, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut payload_bytes__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::PayloadBytes => {
                            if payload_bytes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("payloadBytes"));
                            }
                            payload_bytes__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(send_message_data::V1 {
                    payload_bytes: payload_bytes__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.SendMessageData.V1", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for UpdateAdminListsData {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.UpdateAdminListsData", len)?;
        if let Some(v) = self.version.as_ref() {
            match v {
                update_admin_lists_data::Version::V1(v) => {
                    struct_ser.serialize_field("v1", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for UpdateAdminListsData {
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
            type Value = UpdateAdminListsData;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.UpdateAdminListsData")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<UpdateAdminListsData, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut version__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::V1 => {
                            if version__.is_some() {
                                return Err(serde::de::Error::duplicate_field("v1"));
                            }
                            version__ = map_.next_value::<::std::option::Option<_>>()?.map(update_admin_lists_data::Version::V1)
;
                        }
                    }
                }
                Ok(UpdateAdminListsData {
                    version: version__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.UpdateAdminListsData", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for update_admin_lists_data::V1 {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.admin_list_update_type != 0 {
            len += 1;
        }
        if !self.inbox_id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.UpdateAdminListsData.V1", len)?;
        if self.admin_list_update_type != 0 {
            let v = AdminListUpdateType::try_from(self.admin_list_update_type)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.admin_list_update_type)))?;
            struct_ser.serialize_field("adminListUpdateType", &v)?;
        }
        if !self.inbox_id.is_empty() {
            struct_ser.serialize_field("inboxId", &self.inbox_id)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for update_admin_lists_data::V1 {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "admin_list_update_type",
            "adminListUpdateType",
            "inbox_id",
            "inboxId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AdminListUpdateType,
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
                            "adminListUpdateType" | "admin_list_update_type" => Ok(GeneratedField::AdminListUpdateType),
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
            type Value = update_admin_lists_data::V1;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.UpdateAdminListsData.V1")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<update_admin_lists_data::V1, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut admin_list_update_type__ = None;
                let mut inbox_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AdminListUpdateType => {
                            if admin_list_update_type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("adminListUpdateType"));
                            }
                            admin_list_update_type__ = Some(map_.next_value::<AdminListUpdateType>()? as i32);
                        }
                        GeneratedField::InboxId => {
                            if inbox_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inboxId"));
                            }
                            inbox_id__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(update_admin_lists_data::V1 {
                    admin_list_update_type: admin_list_update_type__.unwrap_or_default(),
                    inbox_id: inbox_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.UpdateAdminListsData.V1", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for UpdateGroupMembershipData {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.UpdateGroupMembershipData", len)?;
        if let Some(v) = self.version.as_ref() {
            match v {
                update_group_membership_data::Version::V1(v) => {
                    struct_ser.serialize_field("v1", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for UpdateGroupMembershipData {
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
            type Value = UpdateGroupMembershipData;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.UpdateGroupMembershipData")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<UpdateGroupMembershipData, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut version__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::V1 => {
                            if version__.is_some() {
                                return Err(serde::de::Error::duplicate_field("v1"));
                            }
                            version__ = map_.next_value::<::std::option::Option<_>>()?.map(update_group_membership_data::Version::V1)
;
                        }
                    }
                }
                Ok(UpdateGroupMembershipData {
                    version: version__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.UpdateGroupMembershipData", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for update_group_membership_data::V1 {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.membership_updates.is_empty() {
            len += 1;
        }
        if !self.removed_members.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.UpdateGroupMembershipData.V1", len)?;
        if !self.membership_updates.is_empty() {
            let v: std::collections::HashMap<_, _> = self.membership_updates.iter()
                .map(|(k, v)| (k, v.to_string())).collect();
            struct_ser.serialize_field("membershipUpdates", &v)?;
        }
        if !self.removed_members.is_empty() {
            struct_ser.serialize_field("removedMembers", &self.removed_members)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for update_group_membership_data::V1 {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "membership_updates",
            "membershipUpdates",
            "removed_members",
            "removedMembers",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            MembershipUpdates,
            RemovedMembers,
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
                            "membershipUpdates" | "membership_updates" => Ok(GeneratedField::MembershipUpdates),
                            "removedMembers" | "removed_members" => Ok(GeneratedField::RemovedMembers),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = update_group_membership_data::V1;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.UpdateGroupMembershipData.V1")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<update_group_membership_data::V1, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut membership_updates__ = None;
                let mut removed_members__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::MembershipUpdates => {
                            if membership_updates__.is_some() {
                                return Err(serde::de::Error::duplicate_field("membershipUpdates"));
                            }
                            membership_updates__ = Some(
                                map_.next_value::<std::collections::HashMap<_, ::pbjson::private::NumberDeserialize<u64>>>()?
                                    .into_iter().map(|(k,v)| (k, v.0)).collect()
                            );
                        }
                        GeneratedField::RemovedMembers => {
                            if removed_members__.is_some() {
                                return Err(serde::de::Error::duplicate_field("removedMembers"));
                            }
                            removed_members__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(update_group_membership_data::V1 {
                    membership_updates: membership_updates__.unwrap_or_default(),
                    removed_members: removed_members__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.UpdateGroupMembershipData.V1", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for UpdateMetadataData {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.UpdateMetadataData", len)?;
        if let Some(v) = self.version.as_ref() {
            match v {
                update_metadata_data::Version::V1(v) => {
                    struct_ser.serialize_field("v1", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for UpdateMetadataData {
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
            type Value = UpdateMetadataData;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.UpdateMetadataData")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<UpdateMetadataData, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut version__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::V1 => {
                            if version__.is_some() {
                                return Err(serde::de::Error::duplicate_field("v1"));
                            }
                            version__ = map_.next_value::<::std::option::Option<_>>()?.map(update_metadata_data::Version::V1)
;
                        }
                    }
                }
                Ok(UpdateMetadataData {
                    version: version__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.UpdateMetadataData", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for update_metadata_data::V1 {
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
        if !self.field_value.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.UpdateMetadataData.V1", len)?;
        if !self.field_name.is_empty() {
            struct_ser.serialize_field("fieldName", &self.field_name)?;
        }
        if !self.field_value.is_empty() {
            struct_ser.serialize_field("fieldValue", &self.field_value)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for update_metadata_data::V1 {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "field_name",
            "fieldName",
            "field_value",
            "fieldValue",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            FieldName,
            FieldValue,
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
                            "fieldValue" | "field_value" => Ok(GeneratedField::FieldValue),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = update_metadata_data::V1;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.UpdateMetadataData.V1")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<update_metadata_data::V1, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut field_name__ = None;
                let mut field_value__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::FieldName => {
                            if field_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fieldName"));
                            }
                            field_name__ = Some(map_.next_value()?);
                        }
                        GeneratedField::FieldValue => {
                            if field_value__.is_some() {
                                return Err(serde::de::Error::duplicate_field("fieldValue"));
                            }
                            field_value__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(update_metadata_data::V1 {
                    field_name: field_name__.unwrap_or_default(),
                    field_value: field_value__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.UpdateMetadataData.V1", FIELDS, GeneratedVisitor)
    }
}
