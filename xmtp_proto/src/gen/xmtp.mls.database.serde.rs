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
        if !self.group_name.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.UpdateMetadataData.V1", len)?;
        if !self.group_name.is_empty() {
            struct_ser.serialize_field("groupName", &self.group_name)?;
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
            "group_name",
            "groupName",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            GroupName,
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
                            "groupName" | "group_name" => Ok(GeneratedField::GroupName),
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
                let mut group_name__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::GroupName => {
                            if group_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("groupName"));
                            }
                            group_name__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(update_metadata_data::V1 {
                    group_name: group_name__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.UpdateMetadataData.V1", FIELDS, GeneratedVisitor)
    }
}
