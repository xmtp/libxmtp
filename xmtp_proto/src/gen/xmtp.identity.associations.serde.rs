// @generated
impl serde::Serialize for AddAssociation {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.new_member_identifier.is_some() {
            len += 1;
        }
        if self.existing_member_signature.is_some() {
            len += 1;
        }
        if self.new_member_signature.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.identity.associations.AddAssociation", len)?;
        if let Some(v) = self.new_member_identifier.as_ref() {
            struct_ser.serialize_field("newMemberIdentifier", v)?;
        }
        if let Some(v) = self.existing_member_signature.as_ref() {
            struct_ser.serialize_field("existingMemberSignature", v)?;
        }
        if let Some(v) = self.new_member_signature.as_ref() {
            struct_ser.serialize_field("newMemberSignature", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AddAssociation {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "new_member_identifier",
            "newMemberIdentifier",
            "existing_member_signature",
            "existingMemberSignature",
            "new_member_signature",
            "newMemberSignature",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            NewMemberIdentifier,
            ExistingMemberSignature,
            NewMemberSignature,
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
                            "newMemberIdentifier" | "new_member_identifier" => Ok(GeneratedField::NewMemberIdentifier),
                            "existingMemberSignature" | "existing_member_signature" => Ok(GeneratedField::ExistingMemberSignature),
                            "newMemberSignature" | "new_member_signature" => Ok(GeneratedField::NewMemberSignature),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AddAssociation;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.identity.associations.AddAssociation")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AddAssociation, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut new_member_identifier__ = None;
                let mut existing_member_signature__ = None;
                let mut new_member_signature__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::NewMemberIdentifier => {
                            if new_member_identifier__.is_some() {
                                return Err(serde::de::Error::duplicate_field("newMemberIdentifier"));
                            }
                            new_member_identifier__ = map_.next_value()?;
                        }
                        GeneratedField::ExistingMemberSignature => {
                            if existing_member_signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("existingMemberSignature"));
                            }
                            existing_member_signature__ = map_.next_value()?;
                        }
                        GeneratedField::NewMemberSignature => {
                            if new_member_signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("newMemberSignature"));
                            }
                            new_member_signature__ = map_.next_value()?;
                        }
                    }
                }
                Ok(AddAssociation {
                    new_member_identifier: new_member_identifier__,
                    existing_member_signature: existing_member_signature__,
                    new_member_signature: new_member_signature__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.identity.associations.AddAssociation", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ChangeRecoveryAddress {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.new_recovery_address.is_empty() {
            len += 1;
        }
        if self.existing_recovery_address_signature.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.identity.associations.ChangeRecoveryAddress", len)?;
        if !self.new_recovery_address.is_empty() {
            struct_ser.serialize_field("newRecoveryAddress", &self.new_recovery_address)?;
        }
        if let Some(v) = self.existing_recovery_address_signature.as_ref() {
            struct_ser.serialize_field("existingRecoveryAddressSignature", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ChangeRecoveryAddress {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "new_recovery_address",
            "newRecoveryAddress",
            "existing_recovery_address_signature",
            "existingRecoveryAddressSignature",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            NewRecoveryAddress,
            ExistingRecoveryAddressSignature,
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
                            "newRecoveryAddress" | "new_recovery_address" => Ok(GeneratedField::NewRecoveryAddress),
                            "existingRecoveryAddressSignature" | "existing_recovery_address_signature" => Ok(GeneratedField::ExistingRecoveryAddressSignature),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ChangeRecoveryAddress;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.identity.associations.ChangeRecoveryAddress")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ChangeRecoveryAddress, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut new_recovery_address__ = None;
                let mut existing_recovery_address_signature__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::NewRecoveryAddress => {
                            if new_recovery_address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("newRecoveryAddress"));
                            }
                            new_recovery_address__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ExistingRecoveryAddressSignature => {
                            if existing_recovery_address_signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("existingRecoveryAddressSignature"));
                            }
                            existing_recovery_address_signature__ = map_.next_value()?;
                        }
                    }
                }
                Ok(ChangeRecoveryAddress {
                    new_recovery_address: new_recovery_address__.unwrap_or_default(),
                    existing_recovery_address_signature: existing_recovery_address_signature__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.identity.associations.ChangeRecoveryAddress", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for CreateInbox {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.initial_address.is_empty() {
            len += 1;
        }
        if self.nonce != 0 {
            len += 1;
        }
        if self.initial_address_signature.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.identity.associations.CreateInbox", len)?;
        if !self.initial_address.is_empty() {
            struct_ser.serialize_field("initialAddress", &self.initial_address)?;
        }
        if self.nonce != 0 {
            #[allow(clippy::needless_borrow)]
            struct_ser.serialize_field("nonce", ToString::to_string(&self.nonce).as_str())?;
        }
        if let Some(v) = self.initial_address_signature.as_ref() {
            struct_ser.serialize_field("initialAddressSignature", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for CreateInbox {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "initial_address",
            "initialAddress",
            "nonce",
            "initial_address_signature",
            "initialAddressSignature",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InitialAddress,
            Nonce,
            InitialAddressSignature,
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
                            "initialAddress" | "initial_address" => Ok(GeneratedField::InitialAddress),
                            "nonce" => Ok(GeneratedField::Nonce),
                            "initialAddressSignature" | "initial_address_signature" => Ok(GeneratedField::InitialAddressSignature),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = CreateInbox;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.identity.associations.CreateInbox")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<CreateInbox, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut initial_address__ = None;
                let mut nonce__ = None;
                let mut initial_address_signature__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::InitialAddress => {
                            if initial_address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("initialAddress"));
                            }
                            initial_address__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Nonce => {
                            if nonce__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nonce"));
                            }
                            nonce__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::InitialAddressSignature => {
                            if initial_address_signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("initialAddressSignature"));
                            }
                            initial_address_signature__ = map_.next_value()?;
                        }
                    }
                }
                Ok(CreateInbox {
                    initial_address: initial_address__.unwrap_or_default(),
                    nonce: nonce__.unwrap_or_default(),
                    initial_address_signature: initial_address_signature__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.identity.associations.CreateInbox", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Erc1271Signature {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.contract_address.is_empty() {
            len += 1;
        }
        if self.block_height != 0 {
            len += 1;
        }
        if !self.signature.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.identity.associations.Erc1271Signature", len)?;
        if !self.contract_address.is_empty() {
            struct_ser.serialize_field("contractAddress", &self.contract_address)?;
        }
        if self.block_height != 0 {
            #[allow(clippy::needless_borrow)]
            struct_ser.serialize_field("blockHeight", ToString::to_string(&self.block_height).as_str())?;
        }
        if !self.signature.is_empty() {
            #[allow(clippy::needless_borrow)]
            struct_ser.serialize_field("signature", pbjson::private::base64::encode(&self.signature).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Erc1271Signature {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "contract_address",
            "contractAddress",
            "block_height",
            "blockHeight",
            "signature",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ContractAddress,
            BlockHeight,
            Signature,
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
                            "contractAddress" | "contract_address" => Ok(GeneratedField::ContractAddress),
                            "blockHeight" | "block_height" => Ok(GeneratedField::BlockHeight),
                            "signature" => Ok(GeneratedField::Signature),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Erc1271Signature;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.identity.associations.Erc1271Signature")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<Erc1271Signature, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut contract_address__ = None;
                let mut block_height__ = None;
                let mut signature__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ContractAddress => {
                            if contract_address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("contractAddress"));
                            }
                            contract_address__ = Some(map_.next_value()?);
                        }
                        GeneratedField::BlockHeight => {
                            if block_height__.is_some() {
                                return Err(serde::de::Error::duplicate_field("blockHeight"));
                            }
                            block_height__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Signature => {
                            if signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("signature"));
                            }
                            signature__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(Erc1271Signature {
                    contract_address: contract_address__.unwrap_or_default(),
                    block_height: block_height__.unwrap_or_default(),
                    signature: signature__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.identity.associations.Erc1271Signature", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for IdentityAction {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.identity.associations.IdentityAction", len)?;
        if let Some(v) = self.kind.as_ref() {
            match v {
                identity_action::Kind::CreateInbox(v) => {
                    struct_ser.serialize_field("createInbox", v)?;
                }
                identity_action::Kind::Add(v) => {
                    struct_ser.serialize_field("add", v)?;
                }
                identity_action::Kind::Revoke(v) => {
                    struct_ser.serialize_field("revoke", v)?;
                }
                identity_action::Kind::ChangeRecoveryAddress(v) => {
                    struct_ser.serialize_field("changeRecoveryAddress", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for IdentityAction {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "create_inbox",
            "createInbox",
            "add",
            "revoke",
            "change_recovery_address",
            "changeRecoveryAddress",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            CreateInbox,
            Add,
            Revoke,
            ChangeRecoveryAddress,
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
                            "createInbox" | "create_inbox" => Ok(GeneratedField::CreateInbox),
                            "add" => Ok(GeneratedField::Add),
                            "revoke" => Ok(GeneratedField::Revoke),
                            "changeRecoveryAddress" | "change_recovery_address" => Ok(GeneratedField::ChangeRecoveryAddress),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = IdentityAction;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.identity.associations.IdentityAction")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<IdentityAction, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut kind__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::CreateInbox => {
                            if kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createInbox"));
                            }
                            kind__ = map_.next_value::<::std::option::Option<_>>()?.map(identity_action::Kind::CreateInbox)
;
                        }
                        GeneratedField::Add => {
                            if kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("add"));
                            }
                            kind__ = map_.next_value::<::std::option::Option<_>>()?.map(identity_action::Kind::Add)
;
                        }
                        GeneratedField::Revoke => {
                            if kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("revoke"));
                            }
                            kind__ = map_.next_value::<::std::option::Option<_>>()?.map(identity_action::Kind::Revoke)
;
                        }
                        GeneratedField::ChangeRecoveryAddress => {
                            if kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("changeRecoveryAddress"));
                            }
                            kind__ = map_.next_value::<::std::option::Option<_>>()?.map(identity_action::Kind::ChangeRecoveryAddress)
;
                        }
                    }
                }
                Ok(IdentityAction {
                    kind: kind__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.identity.associations.IdentityAction", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for IdentityUpdate {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.actions.is_empty() {
            len += 1;
        }
        if self.client_timestamp_ns != 0 {
            len += 1;
        }
        if !self.inbox_id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.identity.associations.IdentityUpdate", len)?;
        if !self.actions.is_empty() {
            struct_ser.serialize_field("actions", &self.actions)?;
        }
        if self.client_timestamp_ns != 0 {
            #[allow(clippy::needless_borrow)]
            struct_ser.serialize_field("clientTimestampNs", ToString::to_string(&self.client_timestamp_ns).as_str())?;
        }
        if !self.inbox_id.is_empty() {
            struct_ser.serialize_field("inboxId", &self.inbox_id)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for IdentityUpdate {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "actions",
            "client_timestamp_ns",
            "clientTimestampNs",
            "inbox_id",
            "inboxId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Actions,
            ClientTimestampNs,
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
                            "actions" => Ok(GeneratedField::Actions),
                            "clientTimestampNs" | "client_timestamp_ns" => Ok(GeneratedField::ClientTimestampNs),
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
            type Value = IdentityUpdate;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.identity.associations.IdentityUpdate")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<IdentityUpdate, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut actions__ = None;
                let mut client_timestamp_ns__ = None;
                let mut inbox_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Actions => {
                            if actions__.is_some() {
                                return Err(serde::de::Error::duplicate_field("actions"));
                            }
                            actions__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ClientTimestampNs => {
                            if client_timestamp_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("clientTimestampNs"));
                            }
                            client_timestamp_ns__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::InboxId => {
                            if inbox_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inboxId"));
                            }
                            inbox_id__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(IdentityUpdate {
                    actions: actions__.unwrap_or_default(),
                    client_timestamp_ns: client_timestamp_ns__.unwrap_or_default(),
                    inbox_id: inbox_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.identity.associations.IdentityUpdate", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for LegacyDelegatedSignature {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.delegated_key.is_some() {
            len += 1;
        }
        if self.signature.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.identity.associations.LegacyDelegatedSignature", len)?;
        if let Some(v) = self.delegated_key.as_ref() {
            struct_ser.serialize_field("delegatedKey", v)?;
        }
        if let Some(v) = self.signature.as_ref() {
            struct_ser.serialize_field("signature", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for LegacyDelegatedSignature {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "delegated_key",
            "delegatedKey",
            "signature",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            DelegatedKey,
            Signature,
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
                            "delegatedKey" | "delegated_key" => Ok(GeneratedField::DelegatedKey),
                            "signature" => Ok(GeneratedField::Signature),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = LegacyDelegatedSignature;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.identity.associations.LegacyDelegatedSignature")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<LegacyDelegatedSignature, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut delegated_key__ = None;
                let mut signature__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::DelegatedKey => {
                            if delegated_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("delegatedKey"));
                            }
                            delegated_key__ = map_.next_value()?;
                        }
                        GeneratedField::Signature => {
                            if signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("signature"));
                            }
                            signature__ = map_.next_value()?;
                        }
                    }
                }
                Ok(LegacyDelegatedSignature {
                    delegated_key: delegated_key__,
                    signature: signature__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.identity.associations.LegacyDelegatedSignature", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Member {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.identifier.is_some() {
            len += 1;
        }
        if self.added_by_entity.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.identity.associations.Member", len)?;
        if let Some(v) = self.identifier.as_ref() {
            struct_ser.serialize_field("identifier", v)?;
        }
        if let Some(v) = self.added_by_entity.as_ref() {
            struct_ser.serialize_field("addedByEntity", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Member {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "identifier",
            "added_by_entity",
            "addedByEntity",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Identifier,
            AddedByEntity,
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
                            "identifier" => Ok(GeneratedField::Identifier),
                            "addedByEntity" | "added_by_entity" => Ok(GeneratedField::AddedByEntity),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Member;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.identity.associations.Member")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<Member, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut identifier__ = None;
                let mut added_by_entity__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Identifier => {
                            if identifier__.is_some() {
                                return Err(serde::de::Error::duplicate_field("identifier"));
                            }
                            identifier__ = map_.next_value()?;
                        }
                        GeneratedField::AddedByEntity => {
                            if added_by_entity__.is_some() {
                                return Err(serde::de::Error::duplicate_field("addedByEntity"));
                            }
                            added_by_entity__ = map_.next_value()?;
                        }
                    }
                }
                Ok(Member {
                    identifier: identifier__,
                    added_by_entity: added_by_entity__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.identity.associations.Member", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for MemberIdentifier {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.identity.associations.MemberIdentifier", len)?;
        if let Some(v) = self.kind.as_ref() {
            match v {
                member_identifier::Kind::Address(v) => {
                    struct_ser.serialize_field("address", v)?;
                }
                member_identifier::Kind::InstallationPublicKey(v) => {
                    #[allow(clippy::needless_borrow)]
                    struct_ser.serialize_field("installationPublicKey", pbjson::private::base64::encode(&v).as_str())?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for MemberIdentifier {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "address",
            "installation_public_key",
            "installationPublicKey",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Address,
            InstallationPublicKey,
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
                            "address" => Ok(GeneratedField::Address),
                            "installationPublicKey" | "installation_public_key" => Ok(GeneratedField::InstallationPublicKey),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = MemberIdentifier;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.identity.associations.MemberIdentifier")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<MemberIdentifier, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut kind__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Address => {
                            if kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("address"));
                            }
                            kind__ = map_.next_value::<::std::option::Option<_>>()?.map(member_identifier::Kind::Address);
                        }
                        GeneratedField::InstallationPublicKey => {
                            if kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationPublicKey"));
                            }
                            kind__ = map_.next_value::<::std::option::Option<::pbjson::private::BytesDeserialize<_>>>()?.map(|x| member_identifier::Kind::InstallationPublicKey(x.0));
                        }
                    }
                }
                Ok(MemberIdentifier {
                    kind: kind__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.identity.associations.MemberIdentifier", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("xmtp.identity.associations.RecoverableEcdsaSignature", len)?;
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
                formatter.write_str("struct xmtp.identity.associations.RecoverableEcdsaSignature")
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
        deserializer.deserialize_struct("xmtp.identity.associations.RecoverableEcdsaSignature", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RecoverableEd25519Signature {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.identity.associations.RecoverableEd25519Signature", len)?;
        if !self.bytes.is_empty() {
            #[allow(clippy::needless_borrow)]
            struct_ser.serialize_field("bytes", pbjson::private::base64::encode(&self.bytes).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RecoverableEd25519Signature {
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
            type Value = RecoverableEd25519Signature;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.identity.associations.RecoverableEd25519Signature")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<RecoverableEd25519Signature, V::Error>
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
                Ok(RecoverableEd25519Signature {
                    bytes: bytes__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.identity.associations.RecoverableEd25519Signature", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RevokeAssociation {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.member_to_revoke.is_some() {
            len += 1;
        }
        if self.recovery_address_signature.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.identity.associations.RevokeAssociation", len)?;
        if let Some(v) = self.member_to_revoke.as_ref() {
            struct_ser.serialize_field("memberToRevoke", v)?;
        }
        if let Some(v) = self.recovery_address_signature.as_ref() {
            struct_ser.serialize_field("recoveryAddressSignature", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RevokeAssociation {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "member_to_revoke",
            "memberToRevoke",
            "recovery_address_signature",
            "recoveryAddressSignature",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            MemberToRevoke,
            RecoveryAddressSignature,
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
                            "memberToRevoke" | "member_to_revoke" => Ok(GeneratedField::MemberToRevoke),
                            "recoveryAddressSignature" | "recovery_address_signature" => Ok(GeneratedField::RecoveryAddressSignature),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = RevokeAssociation;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.identity.associations.RevokeAssociation")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<RevokeAssociation, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut member_to_revoke__ = None;
                let mut recovery_address_signature__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::MemberToRevoke => {
                            if member_to_revoke__.is_some() {
                                return Err(serde::de::Error::duplicate_field("memberToRevoke"));
                            }
                            member_to_revoke__ = map_.next_value()?;
                        }
                        GeneratedField::RecoveryAddressSignature => {
                            if recovery_address_signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("recoveryAddressSignature"));
                            }
                            recovery_address_signature__ = map_.next_value()?;
                        }
                    }
                }
                Ok(RevokeAssociation {
                    member_to_revoke: member_to_revoke__,
                    recovery_address_signature: recovery_address_signature__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.identity.associations.RevokeAssociation", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Signature {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.identity.associations.Signature", len)?;
        if let Some(v) = self.signature.as_ref() {
            match v {
                signature::Signature::Erc191(v) => {
                    struct_ser.serialize_field("erc191", v)?;
                }
                signature::Signature::Erc1271(v) => {
                    struct_ser.serialize_field("erc1271", v)?;
                }
                signature::Signature::InstallationKey(v) => {
                    struct_ser.serialize_field("installationKey", v)?;
                }
                signature::Signature::DelegatedErc191(v) => {
                    struct_ser.serialize_field("delegatedErc191", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Signature {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "erc_191",
            "erc191",
            "erc_1271",
            "erc1271",
            "installation_key",
            "installationKey",
            "delegated_erc_191",
            "delegatedErc191",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Erc191,
            Erc1271,
            InstallationKey,
            DelegatedErc191,
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
                            "erc191" | "erc_191" => Ok(GeneratedField::Erc191),
                            "erc1271" | "erc_1271" => Ok(GeneratedField::Erc1271),
                            "installationKey" | "installation_key" => Ok(GeneratedField::InstallationKey),
                            "delegatedErc191" | "delegated_erc_191" => Ok(GeneratedField::DelegatedErc191),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Signature;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.identity.associations.Signature")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<Signature, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut signature__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Erc191 => {
                            if signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("erc191"));
                            }
                            signature__ = map_.next_value::<::std::option::Option<_>>()?.map(signature::Signature::Erc191)
;
                        }
                        GeneratedField::Erc1271 => {
                            if signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("erc1271"));
                            }
                            signature__ = map_.next_value::<::std::option::Option<_>>()?.map(signature::Signature::Erc1271)
;
                        }
                        GeneratedField::InstallationKey => {
                            if signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationKey"));
                            }
                            signature__ = map_.next_value::<::std::option::Option<_>>()?.map(signature::Signature::InstallationKey)
;
                        }
                        GeneratedField::DelegatedErc191 => {
                            if signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("delegatedErc191"));
                            }
                            signature__ = map_.next_value::<::std::option::Option<_>>()?.map(signature::Signature::DelegatedErc191)
;
                        }
                    }
                }
                Ok(Signature {
                    signature: signature__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.identity.associations.Signature", FIELDS, GeneratedVisitor)
    }
}
