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
            struct_ser.serialize_field("account_addresses", &self.account_addresses)?;
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
                            "accountAddresses" | "account_addresses" => Ok(GeneratedField::AccountAddresses),
                            _ => Ok(GeneratedField::__SkipField__),
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
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
                            "v1" => Ok(GeneratedField::V1),
                            _ => Ok(GeneratedField::__SkipField__),
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
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
            struct_ser.serialize_field("addresses_or_installation_ids", v)?;
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
                            "addressesOrInstallationIds" | "addresses_or_installation_ids" => Ok(GeneratedField::AddressesOrInstallationIds),
                            _ => Ok(GeneratedField::__SkipField__),
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
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
                    struct_ser.serialize_field("account_addresses", v)?;
                }
                addresses_or_installation_ids::AddressesOrInstallationIds::InstallationIds(v) => {
                    struct_ser.serialize_field("installation_ids", v)?;
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
                            "accountAddresses" | "account_addresses" => Ok(GeneratedField::AccountAddresses),
                            "installationIds" | "installation_ids" => Ok(GeneratedField::InstallationIds),
                            _ => Ok(GeneratedField::__SkipField__),
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
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
            Self::Unspecified => "ADMIN_LIST_UPDATE_TYPE_UNSPECIFIED",
            Self::AddAdmin => "ADMIN_LIST_UPDATE_TYPE_ADD_ADMIN",
            Self::RemoveAdmin => "ADMIN_LIST_UPDATE_TYPE_REMOVE_ADMIN",
            Self::AddSuperAdmin => "ADMIN_LIST_UPDATE_TYPE_ADD_SUPER_ADMIN",
            Self::RemoveSuperAdmin => "ADMIN_LIST_UPDATE_TYPE_REMOVE_SUPER_ADMIN",
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
            "ADMIN_LIST_UPDATE_TYPE_UNSPECIFIED",
            "ADMIN_LIST_UPDATE_TYPE_ADD_ADMIN",
            "ADMIN_LIST_UPDATE_TYPE_REMOVE_ADMIN",
            "ADMIN_LIST_UPDATE_TYPE_ADD_SUPER_ADMIN",
            "ADMIN_LIST_UPDATE_TYPE_REMOVE_SUPER_ADMIN",
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
                    "ADMIN_LIST_UPDATE_TYPE_UNSPECIFIED" => Ok(AdminListUpdateType::Unspecified),
                    "ADMIN_LIST_UPDATE_TYPE_ADD_ADMIN" => Ok(AdminListUpdateType::AddAdmin),
                    "ADMIN_LIST_UPDATE_TYPE_REMOVE_ADMIN" => Ok(AdminListUpdateType::RemoveAdmin),
                    "ADMIN_LIST_UPDATE_TYPE_ADD_SUPER_ADMIN" => Ok(AdminListUpdateType::AddSuperAdmin),
                    "ADMIN_LIST_UPDATE_TYPE_REMOVE_SUPER_ADMIN" => Ok(AdminListUpdateType::RemoveSuperAdmin),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for CommitPendingProposalsData {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.CommitPendingProposalsData", len)?;
        if let Some(v) = self.version.as_ref() {
            match v {
                commit_pending_proposals_data::Version::V1(v) => {
                    struct_ser.serialize_field("v1", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for CommitPendingProposalsData {
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
                            "v1" => Ok(GeneratedField::V1),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = CommitPendingProposalsData;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.CommitPendingProposalsData")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<CommitPendingProposalsData, V::Error>
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
                            version__ = map_.next_value::<::std::option::Option<_>>()?.map(commit_pending_proposals_data::Version::V1)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(CommitPendingProposalsData {
                    version: version__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.CommitPendingProposalsData", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for commit_pending_proposals_data::V1 {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.proposal_hashes.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.CommitPendingProposalsData.V1", len)?;
        if !self.proposal_hashes.is_empty() {
            struct_ser.serialize_field("proposal_hashes", &self.proposal_hashes.iter().map(pbjson::private::base64::encode).collect::<Vec<_>>())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for commit_pending_proposals_data::V1 {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "proposal_hashes",
            "proposalHashes",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ProposalHashes,
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
                            "proposalHashes" | "proposal_hashes" => Ok(GeneratedField::ProposalHashes),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = commit_pending_proposals_data::V1;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.CommitPendingProposalsData.V1")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<commit_pending_proposals_data::V1, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut proposal_hashes__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ProposalHashes => {
                            if proposal_hashes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("proposalHashes"));
                            }
                            proposal_hashes__ = 
                                Some(map_.next_value::<Vec<::pbjson::private::BytesDeserialize<_>>>()?
                                    .into_iter().map(|x| x.0).collect())
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(commit_pending_proposals_data::V1 {
                    proposal_hashes: proposal_hashes__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.CommitPendingProposalsData.V1", FIELDS, GeneratedVisitor)
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
            struct_ser.serialize_field("installation_ids", &self.installation_ids.iter().map(pbjson::private::base64::encode).collect::<Vec<_>>())?;
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
                            "installationIds" | "installation_ids" => Ok(GeneratedField::InstallationIds),
                            _ => Ok(GeneratedField::__SkipField__),
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
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
impl serde::Serialize for PermissionPolicyOption {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "PERMISSION_POLICY_OPTION_UNSPECIFIED",
            Self::Allow => "PERMISSION_POLICY_OPTION_ALLOW",
            Self::Deny => "PERMISSION_POLICY_OPTION_DENY",
            Self::AdminOnly => "PERMISSION_POLICY_OPTION_ADMIN_ONLY",
            Self::SuperAdminOnly => "PERMISSION_POLICY_OPTION_SUPER_ADMIN_ONLY",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for PermissionPolicyOption {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "PERMISSION_POLICY_OPTION_UNSPECIFIED",
            "PERMISSION_POLICY_OPTION_ALLOW",
            "PERMISSION_POLICY_OPTION_DENY",
            "PERMISSION_POLICY_OPTION_ADMIN_ONLY",
            "PERMISSION_POLICY_OPTION_SUPER_ADMIN_ONLY",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = PermissionPolicyOption;

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
                    "PERMISSION_POLICY_OPTION_UNSPECIFIED" => Ok(PermissionPolicyOption::Unspecified),
                    "PERMISSION_POLICY_OPTION_ALLOW" => Ok(PermissionPolicyOption::Allow),
                    "PERMISSION_POLICY_OPTION_DENY" => Ok(PermissionPolicyOption::Deny),
                    "PERMISSION_POLICY_OPTION_ADMIN_ONLY" => Ok(PermissionPolicyOption::AdminOnly),
                    "PERMISSION_POLICY_OPTION_SUPER_ADMIN_ONLY" => Ok(PermissionPolicyOption::SuperAdminOnly),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for PermissionUpdateType {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "PERMISSION_UPDATE_TYPE_UNSPECIFIED",
            Self::AddMember => "PERMISSION_UPDATE_TYPE_ADD_MEMBER",
            Self::RemoveMember => "PERMISSION_UPDATE_TYPE_REMOVE_MEMBER",
            Self::AddAdmin => "PERMISSION_UPDATE_TYPE_ADD_ADMIN",
            Self::RemoveAdmin => "PERMISSION_UPDATE_TYPE_REMOVE_ADMIN",
            Self::UpdateMetadata => "PERMISSION_UPDATE_TYPE_UPDATE_METADATA",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for PermissionUpdateType {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "PERMISSION_UPDATE_TYPE_UNSPECIFIED",
            "PERMISSION_UPDATE_TYPE_ADD_MEMBER",
            "PERMISSION_UPDATE_TYPE_REMOVE_MEMBER",
            "PERMISSION_UPDATE_TYPE_ADD_ADMIN",
            "PERMISSION_UPDATE_TYPE_REMOVE_ADMIN",
            "PERMISSION_UPDATE_TYPE_UPDATE_METADATA",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = PermissionUpdateType;

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
                    "PERMISSION_UPDATE_TYPE_UNSPECIFIED" => Ok(PermissionUpdateType::Unspecified),
                    "PERMISSION_UPDATE_TYPE_ADD_MEMBER" => Ok(PermissionUpdateType::AddMember),
                    "PERMISSION_UPDATE_TYPE_REMOVE_MEMBER" => Ok(PermissionUpdateType::RemoveMember),
                    "PERMISSION_UPDATE_TYPE_ADD_ADMIN" => Ok(PermissionUpdateType::AddAdmin),
                    "PERMISSION_UPDATE_TYPE_REMOVE_ADMIN" => Ok(PermissionUpdateType::RemoveAdmin),
                    "PERMISSION_UPDATE_TYPE_UPDATE_METADATA" => Ok(PermissionUpdateType::UpdateMetadata),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
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
                    struct_ser.serialize_field("send_welcomes", v)?;
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
                            "sendWelcomes" | "send_welcomes" => Ok(GeneratedField::SendWelcomes),
                            _ => Ok(GeneratedField::__SkipField__),
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
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
        if self.welcome_wrapper_algorithm != 0 {
            len += 1;
        }
        if self.welcome_pointee_encryption_aead_types.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.PostCommitAction.Installation", len)?;
        if !self.installation_key.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("installation_key", pbjson::private::base64::encode(&self.installation_key).as_str())?;
        }
        if !self.hpke_public_key.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("hpke_public_key", pbjson::private::base64::encode(&self.hpke_public_key).as_str())?;
        }
        if self.welcome_wrapper_algorithm != 0 {
            let v = super::message_contents::WelcomeWrapperAlgorithm::try_from(self.welcome_wrapper_algorithm)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.welcome_wrapper_algorithm)))?;
            struct_ser.serialize_field("welcome_wrapper_algorithm", &v)?;
        }
        if let Some(v) = self.welcome_pointee_encryption_aead_types.as_ref() {
            struct_ser.serialize_field("welcome_pointee_encryption_aead_types", v)?;
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
            "welcome_wrapper_algorithm",
            "welcomeWrapperAlgorithm",
            "welcome_pointee_encryption_aead_types",
            "welcomePointeeEncryptionAeadTypes",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InstallationKey,
            HpkePublicKey,
            WelcomeWrapperAlgorithm,
            WelcomePointeeEncryptionAeadTypes,
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
                            "installationKey" | "installation_key" => Ok(GeneratedField::InstallationKey),
                            "hpkePublicKey" | "hpke_public_key" => Ok(GeneratedField::HpkePublicKey),
                            "welcomeWrapperAlgorithm" | "welcome_wrapper_algorithm" => Ok(GeneratedField::WelcomeWrapperAlgorithm),
                            "welcomePointeeEncryptionAeadTypes" | "welcome_pointee_encryption_aead_types" => Ok(GeneratedField::WelcomePointeeEncryptionAeadTypes),
                            _ => Ok(GeneratedField::__SkipField__),
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
                let mut welcome_wrapper_algorithm__ = None;
                let mut welcome_pointee_encryption_aead_types__ = None;
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
                        GeneratedField::WelcomeWrapperAlgorithm => {
                            if welcome_wrapper_algorithm__.is_some() {
                                return Err(serde::de::Error::duplicate_field("welcomeWrapperAlgorithm"));
                            }
                            welcome_wrapper_algorithm__ = Some(map_.next_value::<super::message_contents::WelcomeWrapperAlgorithm>()? as i32);
                        }
                        GeneratedField::WelcomePointeeEncryptionAeadTypes => {
                            if welcome_pointee_encryption_aead_types__.is_some() {
                                return Err(serde::de::Error::duplicate_field("welcomePointeeEncryptionAeadTypes"));
                            }
                            welcome_pointee_encryption_aead_types__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(post_commit_action::Installation {
                    installation_key: installation_key__.unwrap_or_default(),
                    hpke_public_key: hpke_public_key__.unwrap_or_default(),
                    welcome_wrapper_algorithm: welcome_wrapper_algorithm__.unwrap_or_default(),
                    welcome_pointee_encryption_aead_types: welcome_pointee_encryption_aead_types__,
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
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("welcome_message", pbjson::private::base64::encode(&self.welcome_message).as_str())?;
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
                            "installations" => Ok(GeneratedField::Installations),
                            "welcomeMessage" | "welcome_message" => Ok(GeneratedField::WelcomeMessage),
                            _ => Ok(GeneratedField::__SkipField__),
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
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
impl serde::Serialize for ProposeGroupContextExtensionData {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.ProposeGroupContextExtensionData", len)?;
        if let Some(v) = self.version.as_ref() {
            match v {
                propose_group_context_extension_data::Version::V1(v) => {
                    struct_ser.serialize_field("v1", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ProposeGroupContextExtensionData {
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
                            "v1" => Ok(GeneratedField::V1),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ProposeGroupContextExtensionData;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.ProposeGroupContextExtensionData")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ProposeGroupContextExtensionData, V::Error>
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
                            version__ = map_.next_value::<::std::option::Option<_>>()?.map(propose_group_context_extension_data::Version::V1)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ProposeGroupContextExtensionData {
                    version: version__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.ProposeGroupContextExtensionData", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for propose_group_context_extension_data::V1 {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.group_context_extension.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.ProposeGroupContextExtensionData.V1", len)?;
        if !self.group_context_extension.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("group_context_extension", pbjson::private::base64::encode(&self.group_context_extension).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for propose_group_context_extension_data::V1 {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "group_context_extension",
            "groupContextExtension",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            GroupContextExtension,
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
                            "groupContextExtension" | "group_context_extension" => Ok(GeneratedField::GroupContextExtension),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = propose_group_context_extension_data::V1;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.ProposeGroupContextExtensionData.V1")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<propose_group_context_extension_data::V1, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut group_context_extension__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::GroupContextExtension => {
                            if group_context_extension__.is_some() {
                                return Err(serde::de::Error::duplicate_field("groupContextExtension"));
                            }
                            group_context_extension__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(propose_group_context_extension_data::V1 {
                    group_context_extension: group_context_extension__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.ProposeGroupContextExtensionData.V1", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ProposeMemberUpdateData {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.ProposeMemberUpdateData", len)?;
        if let Some(v) = self.version.as_ref() {
            match v {
                propose_member_update_data::Version::V1(v) => {
                    struct_ser.serialize_field("v1", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ProposeMemberUpdateData {
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
                            "v1" => Ok(GeneratedField::V1),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ProposeMemberUpdateData;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.ProposeMemberUpdateData")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ProposeMemberUpdateData, V::Error>
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
                            version__ = map_.next_value::<::std::option::Option<_>>()?.map(propose_member_update_data::Version::V1)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ProposeMemberUpdateData {
                    version: version__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.ProposeMemberUpdateData", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for propose_member_update_data::V1 {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.add_inbox_ids.is_empty() {
            len += 1;
        }
        if !self.remove_inbox_ids.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.ProposeMemberUpdateData.V1", len)?;
        if !self.add_inbox_ids.is_empty() {
            struct_ser.serialize_field("add_inbox_ids", &self.add_inbox_ids.iter().map(pbjson::private::base64::encode).collect::<Vec<_>>())?;
        }
        if !self.remove_inbox_ids.is_empty() {
            struct_ser.serialize_field("remove_inbox_ids", &self.remove_inbox_ids.iter().map(pbjson::private::base64::encode).collect::<Vec<_>>())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for propose_member_update_data::V1 {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "add_inbox_ids",
            "addInboxIds",
            "remove_inbox_ids",
            "removeInboxIds",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AddInboxIds,
            RemoveInboxIds,
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
                            "addInboxIds" | "add_inbox_ids" => Ok(GeneratedField::AddInboxIds),
                            "removeInboxIds" | "remove_inbox_ids" => Ok(GeneratedField::RemoveInboxIds),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = propose_member_update_data::V1;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.ProposeMemberUpdateData.V1")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<propose_member_update_data::V1, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut add_inbox_ids__ = None;
                let mut remove_inbox_ids__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AddInboxIds => {
                            if add_inbox_ids__.is_some() {
                                return Err(serde::de::Error::duplicate_field("addInboxIds"));
                            }
                            add_inbox_ids__ = 
                                Some(map_.next_value::<Vec<::pbjson::private::BytesDeserialize<_>>>()?
                                    .into_iter().map(|x| x.0).collect())
                            ;
                        }
                        GeneratedField::RemoveInboxIds => {
                            if remove_inbox_ids__.is_some() {
                                return Err(serde::de::Error::duplicate_field("removeInboxIds"));
                            }
                            remove_inbox_ids__ = 
                                Some(map_.next_value::<Vec<::pbjson::private::BytesDeserialize<_>>>()?
                                    .into_iter().map(|x| x.0).collect())
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(propose_member_update_data::V1 {
                    add_inbox_ids: add_inbox_ids__.unwrap_or_default(),
                    remove_inbox_ids: remove_inbox_ids__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.ProposeMemberUpdateData.V1", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ReaddInstallationsData {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.ReaddInstallationsData", len)?;
        if let Some(v) = self.version.as_ref() {
            match v {
                readd_installations_data::Version::V1(v) => {
                    struct_ser.serialize_field("v1", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ReaddInstallationsData {
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
                            "v1" => Ok(GeneratedField::V1),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ReaddInstallationsData;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.ReaddInstallationsData")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ReaddInstallationsData, V::Error>
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
                            version__ = map_.next_value::<::std::option::Option<_>>()?.map(readd_installations_data::Version::V1)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ReaddInstallationsData {
                    version: version__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.ReaddInstallationsData", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for readd_installations_data::V1 {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.readded_installations.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.ReaddInstallationsData.V1", len)?;
        if !self.readded_installations.is_empty() {
            struct_ser.serialize_field("readded_installations", &self.readded_installations.iter().map(pbjson::private::base64::encode).collect::<Vec<_>>())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for readd_installations_data::V1 {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "readded_installations",
            "readdedInstallations",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ReaddedInstallations,
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
                            "readdedInstallations" | "readded_installations" => Ok(GeneratedField::ReaddedInstallations),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = readd_installations_data::V1;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.ReaddInstallationsData.V1")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<readd_installations_data::V1, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut readded_installations__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ReaddedInstallations => {
                            if readded_installations__.is_some() {
                                return Err(serde::de::Error::duplicate_field("readdedInstallations"));
                            }
                            readded_installations__ = 
                                Some(map_.next_value::<Vec<::pbjson::private::BytesDeserialize<_>>>()?
                                    .into_iter().map(|x| x.0).collect())
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(readd_installations_data::V1 {
                    readded_installations: readded_installations__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.ReaddInstallationsData.V1", FIELDS, GeneratedVisitor)
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
                            "v1" => Ok(GeneratedField::V1),
                            _ => Ok(GeneratedField::__SkipField__),
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
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
            struct_ser.serialize_field("addresses_or_installation_ids", v)?;
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
                            "addressesOrInstallationIds" | "addresses_or_installation_ids" => Ok(GeneratedField::AddressesOrInstallationIds),
                            _ => Ok(GeneratedField::__SkipField__),
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
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
                            "v1" => Ok(GeneratedField::V1),
                            _ => Ok(GeneratedField::__SkipField__),
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
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
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("payload_bytes", pbjson::private::base64::encode(&self.payload_bytes).as_str())?;
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
                            "payloadBytes" | "payload_bytes" => Ok(GeneratedField::PayloadBytes),
                            _ => Ok(GeneratedField::__SkipField__),
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
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
impl serde::Serialize for SendSyncArchive {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.options.is_some() {
            len += 1;
        }
        if !self.sync_group_id.is_empty() {
            len += 1;
        }
        if self.request_id.is_some() {
            len += 1;
        }
        if !self.server_url.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.SendSyncArchive", len)?;
        if let Some(v) = self.options.as_ref() {
            struct_ser.serialize_field("options", v)?;
        }
        if !self.sync_group_id.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("sync_group_id", pbjson::private::base64::encode(&self.sync_group_id).as_str())?;
        }
        if let Some(v) = self.request_id.as_ref() {
            struct_ser.serialize_field("request_id", v)?;
        }
        if !self.server_url.is_empty() {
            struct_ser.serialize_field("server_url", &self.server_url)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SendSyncArchive {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "options",
            "sync_group_id",
            "syncGroupId",
            "request_id",
            "requestId",
            "server_url",
            "serverUrl",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Options,
            SyncGroupId,
            RequestId,
            ServerUrl,
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
                            "options" => Ok(GeneratedField::Options),
                            "syncGroupId" | "sync_group_id" => Ok(GeneratedField::SyncGroupId),
                            "requestId" | "request_id" => Ok(GeneratedField::RequestId),
                            "serverUrl" | "server_url" => Ok(GeneratedField::ServerUrl),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SendSyncArchive;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.SendSyncArchive")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SendSyncArchive, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut options__ = None;
                let mut sync_group_id__ = None;
                let mut request_id__ = None;
                let mut server_url__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Options => {
                            if options__.is_some() {
                                return Err(serde::de::Error::duplicate_field("options"));
                            }
                            options__ = map_.next_value()?;
                        }
                        GeneratedField::SyncGroupId => {
                            if sync_group_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("syncGroupId"));
                            }
                            sync_group_id__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::RequestId => {
                            if request_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("requestId"));
                            }
                            request_id__ = map_.next_value()?;
                        }
                        GeneratedField::ServerUrl => {
                            if server_url__.is_some() {
                                return Err(serde::de::Error::duplicate_field("serverUrl"));
                            }
                            server_url__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(SendSyncArchive {
                    options: options__,
                    sync_group_id: sync_group_id__.unwrap_or_default(),
                    request_id: request_id__,
                    server_url: server_url__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.SendSyncArchive", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Task {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.task.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.Task", len)?;
        if let Some(v) = self.task.as_ref() {
            match v {
                task::Task::ProcessWelcomePointer(v) => {
                    struct_ser.serialize_field("process_welcome_pointer", v)?;
                }
                task::Task::SendSyncArchive(v) => {
                    struct_ser.serialize_field("send_sync_archive", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Task {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "process_welcome_pointer",
            "processWelcomePointer",
            "send_sync_archive",
            "sendSyncArchive",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ProcessWelcomePointer,
            SendSyncArchive,
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
                            "processWelcomePointer" | "process_welcome_pointer" => Ok(GeneratedField::ProcessWelcomePointer),
                            "sendSyncArchive" | "send_sync_archive" => Ok(GeneratedField::SendSyncArchive),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Task;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.Task")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<Task, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut task__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ProcessWelcomePointer => {
                            if task__.is_some() {
                                return Err(serde::de::Error::duplicate_field("processWelcomePointer"));
                            }
                            task__ = map_.next_value::<::std::option::Option<_>>()?.map(task::Task::ProcessWelcomePointer)
;
                        }
                        GeneratedField::SendSyncArchive => {
                            if task__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sendSyncArchive"));
                            }
                            task__ = map_.next_value::<::std::option::Option<_>>()?.map(task::Task::SendSyncArchive)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(Task {
                    task: task__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.Task", FIELDS, GeneratedVisitor)
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
                            "v1" => Ok(GeneratedField::V1),
                            _ => Ok(GeneratedField::__SkipField__),
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
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
            struct_ser.serialize_field("admin_list_update_type", &v)?;
        }
        if !self.inbox_id.is_empty() {
            struct_ser.serialize_field("inbox_id", &self.inbox_id)?;
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
                            "adminListUpdateType" | "admin_list_update_type" => Ok(GeneratedField::AdminListUpdateType),
                            "inboxId" | "inbox_id" => Ok(GeneratedField::InboxId),
                            _ => Ok(GeneratedField::__SkipField__),
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
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
                            "v1" => Ok(GeneratedField::V1),
                            _ => Ok(GeneratedField::__SkipField__),
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
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
        if !self.failed_installations.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.UpdateGroupMembershipData.V1", len)?;
        if !self.membership_updates.is_empty() {
            let v: std::collections::HashMap<_, _> = self.membership_updates.iter()
                .map(|(k, v)| (k, v.to_string())).collect();
            struct_ser.serialize_field("membership_updates", &v)?;
        }
        if !self.removed_members.is_empty() {
            struct_ser.serialize_field("removed_members", &self.removed_members)?;
        }
        if !self.failed_installations.is_empty() {
            struct_ser.serialize_field("failed_installations", &self.failed_installations.iter().map(pbjson::private::base64::encode).collect::<Vec<_>>())?;
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
            "failed_installations",
            "failedInstallations",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            MembershipUpdates,
            RemovedMembers,
            FailedInstallations,
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
                            "membershipUpdates" | "membership_updates" => Ok(GeneratedField::MembershipUpdates),
                            "removedMembers" | "removed_members" => Ok(GeneratedField::RemovedMembers),
                            "failedInstallations" | "failed_installations" => Ok(GeneratedField::FailedInstallations),
                            _ => Ok(GeneratedField::__SkipField__),
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
                let mut failed_installations__ = None;
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
                        GeneratedField::FailedInstallations => {
                            if failed_installations__.is_some() {
                                return Err(serde::de::Error::duplicate_field("failedInstallations"));
                            }
                            failed_installations__ = 
                                Some(map_.next_value::<Vec<::pbjson::private::BytesDeserialize<_>>>()?
                                    .into_iter().map(|x| x.0).collect())
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(update_group_membership_data::V1 {
                    membership_updates: membership_updates__.unwrap_or_default(),
                    removed_members: removed_members__.unwrap_or_default(),
                    failed_installations: failed_installations__.unwrap_or_default(),
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
                            "v1" => Ok(GeneratedField::V1),
                            _ => Ok(GeneratedField::__SkipField__),
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
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
            struct_ser.serialize_field("field_name", &self.field_name)?;
        }
        if !self.field_value.is_empty() {
            struct_ser.serialize_field("field_value", &self.field_value)?;
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
                            "fieldName" | "field_name" => Ok(GeneratedField::FieldName),
                            "fieldValue" | "field_value" => Ok(GeneratedField::FieldValue),
                            _ => Ok(GeneratedField::__SkipField__),
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
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
impl serde::Serialize for UpdatePermissionData {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.UpdatePermissionData", len)?;
        if let Some(v) = self.version.as_ref() {
            match v {
                update_permission_data::Version::V1(v) => {
                    struct_ser.serialize_field("v1", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for UpdatePermissionData {
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
                            "v1" => Ok(GeneratedField::V1),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = UpdatePermissionData;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.UpdatePermissionData")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<UpdatePermissionData, V::Error>
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
                            version__ = map_.next_value::<::std::option::Option<_>>()?.map(update_permission_data::Version::V1)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(UpdatePermissionData {
                    version: version__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.UpdatePermissionData", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for update_permission_data::V1 {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.permission_update_type != 0 {
            len += 1;
        }
        if self.permission_policy_option != 0 {
            len += 1;
        }
        if self.metadata_field_name.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.mls.database.UpdatePermissionData.V1", len)?;
        if self.permission_update_type != 0 {
            let v = PermissionUpdateType::try_from(self.permission_update_type)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.permission_update_type)))?;
            struct_ser.serialize_field("permission_update_type", &v)?;
        }
        if self.permission_policy_option != 0 {
            let v = PermissionPolicyOption::try_from(self.permission_policy_option)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.permission_policy_option)))?;
            struct_ser.serialize_field("permission_policy_option", &v)?;
        }
        if let Some(v) = self.metadata_field_name.as_ref() {
            struct_ser.serialize_field("metadata_field_name", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for update_permission_data::V1 {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "permission_update_type",
            "permissionUpdateType",
            "permission_policy_option",
            "permissionPolicyOption",
            "metadata_field_name",
            "metadataFieldName",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            PermissionUpdateType,
            PermissionPolicyOption,
            MetadataFieldName,
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
                            "permissionUpdateType" | "permission_update_type" => Ok(GeneratedField::PermissionUpdateType),
                            "permissionPolicyOption" | "permission_policy_option" => Ok(GeneratedField::PermissionPolicyOption),
                            "metadataFieldName" | "metadata_field_name" => Ok(GeneratedField::MetadataFieldName),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = update_permission_data::V1;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.mls.database.UpdatePermissionData.V1")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<update_permission_data::V1, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut permission_update_type__ = None;
                let mut permission_policy_option__ = None;
                let mut metadata_field_name__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::PermissionUpdateType => {
                            if permission_update_type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("permissionUpdateType"));
                            }
                            permission_update_type__ = Some(map_.next_value::<PermissionUpdateType>()? as i32);
                        }
                        GeneratedField::PermissionPolicyOption => {
                            if permission_policy_option__.is_some() {
                                return Err(serde::de::Error::duplicate_field("permissionPolicyOption"));
                            }
                            permission_policy_option__ = Some(map_.next_value::<PermissionPolicyOption>()? as i32);
                        }
                        GeneratedField::MetadataFieldName => {
                            if metadata_field_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("metadataFieldName"));
                            }
                            metadata_field_name__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(update_permission_data::V1 {
                    permission_update_type: permission_update_type__.unwrap_or_default(),
                    permission_policy_option: permission_policy_option__.unwrap_or_default(),
                    metadata_field_name: metadata_field_name__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.mls.database.UpdatePermissionData.V1", FIELDS, GeneratedVisitor)
    }
}
