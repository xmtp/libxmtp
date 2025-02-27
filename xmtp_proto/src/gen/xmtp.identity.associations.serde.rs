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
        if self.relying_partner.is_some() {
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
        if let Some(v) = self.relying_partner.as_ref() {
            struct_ser.serialize_field("relyingPartner", v)?;
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
            "relying_partner",
            "relyingPartner",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            NewMemberIdentifier,
            ExistingMemberSignature,
            NewMemberSignature,
            RelyingPartner,
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
                            "relyingPartner" | "relying_partner" => Ok(GeneratedField::RelyingPartner),
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
                let mut relying_partner__ = None;
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
                        GeneratedField::RelyingPartner => {
                            if relying_partner__.is_some() {
                                return Err(serde::de::Error::duplicate_field("relyingPartner"));
                            }
                            relying_partner__ = map_.next_value()?;
                        }
                    }
                }
                Ok(AddAssociation {
                    new_member_identifier: new_member_identifier__,
                    existing_member_signature: existing_member_signature__,
                    new_member_signature: new_member_signature__,
                    relying_partner: relying_partner__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.identity.associations.AddAssociation", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AssociationState {
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
        if !self.members.is_empty() {
            len += 1;
        }
        if !self.recovery_identifier.is_empty() {
            len += 1;
        }
        if !self.seen_signatures.is_empty() {
            len += 1;
        }
        if self.recovery_identifier_kind != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.identity.associations.AssociationState", len)?;
        if !self.inbox_id.is_empty() {
            struct_ser.serialize_field("inboxId", &self.inbox_id)?;
        }
        if !self.members.is_empty() {
            struct_ser.serialize_field("members", &self.members)?;
        }
        if !self.recovery_identifier.is_empty() {
            struct_ser.serialize_field("recoveryIdentifier", &self.recovery_identifier)?;
        }
        if !self.seen_signatures.is_empty() {
            struct_ser.serialize_field("seenSignatures", &self.seen_signatures.iter().map(pbjson::private::base64::encode).collect::<Vec<_>>())?;
        }
        if self.recovery_identifier_kind != 0 {
            let v = IdentifierKind::try_from(self.recovery_identifier_kind)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.recovery_identifier_kind)))?;
            struct_ser.serialize_field("recoveryIdentifierKind", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AssociationState {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "inbox_id",
            "inboxId",
            "members",
            "recovery_identifier",
            "recoveryIdentifier",
            "seen_signatures",
            "seenSignatures",
            "recovery_identifier_kind",
            "recoveryIdentifierKind",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InboxId,
            Members,
            RecoveryIdentifier,
            SeenSignatures,
            RecoveryIdentifierKind,
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
                            "members" => Ok(GeneratedField::Members),
                            "recoveryIdentifier" | "recovery_identifier" => Ok(GeneratedField::RecoveryIdentifier),
                            "seenSignatures" | "seen_signatures" => Ok(GeneratedField::SeenSignatures),
                            "recoveryIdentifierKind" | "recovery_identifier_kind" => Ok(GeneratedField::RecoveryIdentifierKind),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AssociationState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.identity.associations.AssociationState")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AssociationState, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut inbox_id__ = None;
                let mut members__ = None;
                let mut recovery_identifier__ = None;
                let mut seen_signatures__ = None;
                let mut recovery_identifier_kind__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::InboxId => {
                            if inbox_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inboxId"));
                            }
                            inbox_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Members => {
                            if members__.is_some() {
                                return Err(serde::de::Error::duplicate_field("members"));
                            }
                            members__ = Some(map_.next_value()?);
                        }
                        GeneratedField::RecoveryIdentifier => {
                            if recovery_identifier__.is_some() {
                                return Err(serde::de::Error::duplicate_field("recoveryIdentifier"));
                            }
                            recovery_identifier__ = Some(map_.next_value()?);
                        }
                        GeneratedField::SeenSignatures => {
                            if seen_signatures__.is_some() {
                                return Err(serde::de::Error::duplicate_field("seenSignatures"));
                            }
                            seen_signatures__ = 
                                Some(map_.next_value::<Vec<::pbjson::private::BytesDeserialize<_>>>()?
                                    .into_iter().map(|x| x.0).collect())
                            ;
                        }
                        GeneratedField::RecoveryIdentifierKind => {
                            if recovery_identifier_kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("recoveryIdentifierKind"));
                            }
                            recovery_identifier_kind__ = Some(map_.next_value::<IdentifierKind>()? as i32);
                        }
                    }
                }
                Ok(AssociationState {
                    inbox_id: inbox_id__.unwrap_or_default(),
                    members: members__.unwrap_or_default(),
                    recovery_identifier: recovery_identifier__.unwrap_or_default(),
                    seen_signatures: seen_signatures__.unwrap_or_default(),
                    recovery_identifier_kind: recovery_identifier_kind__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.identity.associations.AssociationState", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AssociationStateDiff {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.new_members.is_empty() {
            len += 1;
        }
        if !self.removed_members.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.identity.associations.AssociationStateDiff", len)?;
        if !self.new_members.is_empty() {
            struct_ser.serialize_field("newMembers", &self.new_members)?;
        }
        if !self.removed_members.is_empty() {
            struct_ser.serialize_field("removedMembers", &self.removed_members)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AssociationStateDiff {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "new_members",
            "newMembers",
            "removed_members",
            "removedMembers",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            NewMembers,
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
                            "newMembers" | "new_members" => Ok(GeneratedField::NewMembers),
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
            type Value = AssociationStateDiff;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.identity.associations.AssociationStateDiff")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AssociationStateDiff, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut new_members__ = None;
                let mut removed_members__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::NewMembers => {
                            if new_members__.is_some() {
                                return Err(serde::de::Error::duplicate_field("newMembers"));
                            }
                            new_members__ = Some(map_.next_value()?);
                        }
                        GeneratedField::RemovedMembers => {
                            if removed_members__.is_some() {
                                return Err(serde::de::Error::duplicate_field("removedMembers"));
                            }
                            removed_members__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(AssociationStateDiff {
                    new_members: new_members__.unwrap_or_default(),
                    removed_members: removed_members__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.identity.associations.AssociationStateDiff", FIELDS, GeneratedVisitor)
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
        if !self.new_recovery_identifier.is_empty() {
            len += 1;
        }
        if self.existing_recovery_identifier_signature.is_some() {
            len += 1;
        }
        if self.new_recovery_identifier_kind != 0 {
            len += 1;
        }
        if self.relying_partner.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.identity.associations.ChangeRecoveryAddress", len)?;
        if !self.new_recovery_identifier.is_empty() {
            struct_ser.serialize_field("newRecoveryIdentifier", &self.new_recovery_identifier)?;
        }
        if let Some(v) = self.existing_recovery_identifier_signature.as_ref() {
            struct_ser.serialize_field("existingRecoveryIdentifierSignature", v)?;
        }
        if self.new_recovery_identifier_kind != 0 {
            let v = IdentifierKind::try_from(self.new_recovery_identifier_kind)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.new_recovery_identifier_kind)))?;
            struct_ser.serialize_field("newRecoveryIdentifierKind", &v)?;
        }
        if let Some(v) = self.relying_partner.as_ref() {
            struct_ser.serialize_field("relyingPartner", v)?;
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
            "new_recovery_identifier",
            "newRecoveryIdentifier",
            "existing_recovery_identifier_signature",
            "existingRecoveryIdentifierSignature",
            "new_recovery_identifier_kind",
            "newRecoveryIdentifierKind",
            "relying_partner",
            "relyingPartner",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            NewRecoveryIdentifier,
            ExistingRecoveryIdentifierSignature,
            NewRecoveryIdentifierKind,
            RelyingPartner,
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
                            "newRecoveryIdentifier" | "new_recovery_identifier" => Ok(GeneratedField::NewRecoveryIdentifier),
                            "existingRecoveryIdentifierSignature" | "existing_recovery_identifier_signature" => Ok(GeneratedField::ExistingRecoveryIdentifierSignature),
                            "newRecoveryIdentifierKind" | "new_recovery_identifier_kind" => Ok(GeneratedField::NewRecoveryIdentifierKind),
                            "relyingPartner" | "relying_partner" => Ok(GeneratedField::RelyingPartner),
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
                let mut new_recovery_identifier__ = None;
                let mut existing_recovery_identifier_signature__ = None;
                let mut new_recovery_identifier_kind__ = None;
                let mut relying_partner__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::NewRecoveryIdentifier => {
                            if new_recovery_identifier__.is_some() {
                                return Err(serde::de::Error::duplicate_field("newRecoveryIdentifier"));
                            }
                            new_recovery_identifier__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ExistingRecoveryIdentifierSignature => {
                            if existing_recovery_identifier_signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("existingRecoveryIdentifierSignature"));
                            }
                            existing_recovery_identifier_signature__ = map_.next_value()?;
                        }
                        GeneratedField::NewRecoveryIdentifierKind => {
                            if new_recovery_identifier_kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("newRecoveryIdentifierKind"));
                            }
                            new_recovery_identifier_kind__ = Some(map_.next_value::<IdentifierKind>()? as i32);
                        }
                        GeneratedField::RelyingPartner => {
                            if relying_partner__.is_some() {
                                return Err(serde::de::Error::duplicate_field("relyingPartner"));
                            }
                            relying_partner__ = map_.next_value()?;
                        }
                    }
                }
                Ok(ChangeRecoveryAddress {
                    new_recovery_identifier: new_recovery_identifier__.unwrap_or_default(),
                    existing_recovery_identifier_signature: existing_recovery_identifier_signature__,
                    new_recovery_identifier_kind: new_recovery_identifier_kind__.unwrap_or_default(),
                    relying_partner: relying_partner__,
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
        if !self.initial_identifier.is_empty() {
            len += 1;
        }
        if self.nonce != 0 {
            len += 1;
        }
        if self.initial_identifier_signature.is_some() {
            len += 1;
        }
        if self.initial_identifier_kind != 0 {
            len += 1;
        }
        if self.relying_partner.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.identity.associations.CreateInbox", len)?;
        if !self.initial_identifier.is_empty() {
            struct_ser.serialize_field("initialIdentifier", &self.initial_identifier)?;
        }
        if self.nonce != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("nonce", ToString::to_string(&self.nonce).as_str())?;
        }
        if let Some(v) = self.initial_identifier_signature.as_ref() {
            struct_ser.serialize_field("initialIdentifierSignature", v)?;
        }
        if self.initial_identifier_kind != 0 {
            let v = IdentifierKind::try_from(self.initial_identifier_kind)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.initial_identifier_kind)))?;
            struct_ser.serialize_field("initialIdentifierKind", &v)?;
        }
        if let Some(v) = self.relying_partner.as_ref() {
            struct_ser.serialize_field("relyingPartner", v)?;
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
            "initial_identifier",
            "initialIdentifier",
            "nonce",
            "initial_identifier_signature",
            "initialIdentifierSignature",
            "initial_identifier_kind",
            "initialIdentifierKind",
            "relying_partner",
            "relyingPartner",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            InitialIdentifier,
            Nonce,
            InitialIdentifierSignature,
            InitialIdentifierKind,
            RelyingPartner,
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
                            "initialIdentifier" | "initial_identifier" => Ok(GeneratedField::InitialIdentifier),
                            "nonce" => Ok(GeneratedField::Nonce),
                            "initialIdentifierSignature" | "initial_identifier_signature" => Ok(GeneratedField::InitialIdentifierSignature),
                            "initialIdentifierKind" | "initial_identifier_kind" => Ok(GeneratedField::InitialIdentifierKind),
                            "relyingPartner" | "relying_partner" => Ok(GeneratedField::RelyingPartner),
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
                let mut initial_identifier__ = None;
                let mut nonce__ = None;
                let mut initial_identifier_signature__ = None;
                let mut initial_identifier_kind__ = None;
                let mut relying_partner__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::InitialIdentifier => {
                            if initial_identifier__.is_some() {
                                return Err(serde::de::Error::duplicate_field("initialIdentifier"));
                            }
                            initial_identifier__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Nonce => {
                            if nonce__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nonce"));
                            }
                            nonce__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::InitialIdentifierSignature => {
                            if initial_identifier_signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("initialIdentifierSignature"));
                            }
                            initial_identifier_signature__ = map_.next_value()?;
                        }
                        GeneratedField::InitialIdentifierKind => {
                            if initial_identifier_kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("initialIdentifierKind"));
                            }
                            initial_identifier_kind__ = Some(map_.next_value::<IdentifierKind>()? as i32);
                        }
                        GeneratedField::RelyingPartner => {
                            if relying_partner__.is_some() {
                                return Err(serde::de::Error::duplicate_field("relyingPartner"));
                            }
                            relying_partner__ = map_.next_value()?;
                        }
                    }
                }
                Ok(CreateInbox {
                    initial_identifier: initial_identifier__.unwrap_or_default(),
                    nonce: nonce__.unwrap_or_default(),
                    initial_identifier_signature: initial_identifier_signature__,
                    initial_identifier_kind: initial_identifier_kind__.unwrap_or_default(),
                    relying_partner: relying_partner__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.identity.associations.CreateInbox", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for IdentifierKind {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "IDENTIFIER_KIND_UNSPECIFIED",
            Self::Ethereum => "IDENTIFIER_KIND_ETHEREUM",
            Self::Passkey => "IDENTIFIER_KIND_PASSKEY",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for IdentifierKind {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "IDENTIFIER_KIND_UNSPECIFIED",
            "IDENTIFIER_KIND_ETHEREUM",
            "IDENTIFIER_KIND_PASSKEY",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = IdentifierKind;

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
                    "IDENTIFIER_KIND_UNSPECIFIED" => Ok(IdentifierKind::Unspecified),
                    "IDENTIFIER_KIND_ETHEREUM" => Ok(IdentifierKind::Ethereum),
                    "IDENTIFIER_KIND_PASSKEY" => Ok(IdentifierKind::Passkey),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
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
            #[allow(clippy::needless_borrows_for_generic_args)]
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
        if self.client_timestamp_ns.is_some() {
            len += 1;
        }
        if self.added_on_chain_id.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.identity.associations.Member", len)?;
        if let Some(v) = self.identifier.as_ref() {
            struct_ser.serialize_field("identifier", v)?;
        }
        if let Some(v) = self.added_by_entity.as_ref() {
            struct_ser.serialize_field("addedByEntity", v)?;
        }
        if let Some(v) = self.client_timestamp_ns.as_ref() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("clientTimestampNs", ToString::to_string(&v).as_str())?;
        }
        if let Some(v) = self.added_on_chain_id.as_ref() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("addedOnChainId", ToString::to_string(&v).as_str())?;
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
            "client_timestamp_ns",
            "clientTimestampNs",
            "added_on_chain_id",
            "addedOnChainId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Identifier,
            AddedByEntity,
            ClientTimestampNs,
            AddedOnChainId,
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
                            "clientTimestampNs" | "client_timestamp_ns" => Ok(GeneratedField::ClientTimestampNs),
                            "addedOnChainId" | "added_on_chain_id" => Ok(GeneratedField::AddedOnChainId),
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
                let mut client_timestamp_ns__ = None;
                let mut added_on_chain_id__ = None;
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
                        GeneratedField::ClientTimestampNs => {
                            if client_timestamp_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("clientTimestampNs"));
                            }
                            client_timestamp_ns__ = 
                                map_.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| x.0)
                            ;
                        }
                        GeneratedField::AddedOnChainId => {
                            if added_on_chain_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("addedOnChainId"));
                            }
                            added_on_chain_id__ = 
                                map_.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| x.0)
                            ;
                        }
                    }
                }
                Ok(Member {
                    identifier: identifier__,
                    added_by_entity: added_by_entity__,
                    client_timestamp_ns: client_timestamp_ns__,
                    added_on_chain_id: added_on_chain_id__,
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
                member_identifier::Kind::EthereumAddress(v) => {
                    struct_ser.serialize_field("ethereumAddress", v)?;
                }
                member_identifier::Kind::InstallationPublicKey(v) => {
                    #[allow(clippy::needless_borrow)]
                    #[allow(clippy::needless_borrows_for_generic_args)]
                    struct_ser.serialize_field("installationPublicKey", pbjson::private::base64::encode(&v).as_str())?;
                }
                member_identifier::Kind::Passkey(v) => {
                    struct_ser.serialize_field("passkey", v)?;
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
            "ethereum_address",
            "ethereumAddress",
            "installation_public_key",
            "installationPublicKey",
            "passkey",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            EthereumAddress,
            InstallationPublicKey,
            Passkey,
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
                            "ethereumAddress" | "ethereum_address" => Ok(GeneratedField::EthereumAddress),
                            "installationPublicKey" | "installation_public_key" => Ok(GeneratedField::InstallationPublicKey),
                            "passkey" => Ok(GeneratedField::Passkey),
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
                        GeneratedField::EthereumAddress => {
                            if kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ethereumAddress"));
                            }
                            kind__ = map_.next_value::<::std::option::Option<_>>()?.map(member_identifier::Kind::EthereumAddress);
                        }
                        GeneratedField::InstallationPublicKey => {
                            if kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("installationPublicKey"));
                            }
                            kind__ = map_.next_value::<::std::option::Option<::pbjson::private::BytesDeserialize<_>>>()?.map(|x| member_identifier::Kind::InstallationPublicKey(x.0));
                        }
                        GeneratedField::Passkey => {
                            if kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("passkey"));
                            }
                            kind__ = map_.next_value::<::std::option::Option<_>>()?.map(member_identifier::Kind::Passkey)
;
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
impl serde::Serialize for MemberMap {
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
        if self.value.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.identity.associations.MemberMap", len)?;
        if let Some(v) = self.key.as_ref() {
            struct_ser.serialize_field("key", v)?;
        }
        if let Some(v) = self.value.as_ref() {
            struct_ser.serialize_field("value", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for MemberMap {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "key",
            "value",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Key,
            Value,
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
                            "value" => Ok(GeneratedField::Value),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = MemberMap;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.identity.associations.MemberMap")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<MemberMap, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut key__ = None;
                let mut value__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Key => {
                            if key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("key"));
                            }
                            key__ = map_.next_value()?;
                        }
                        GeneratedField::Value => {
                            if value__.is_some() {
                                return Err(serde::de::Error::duplicate_field("value"));
                            }
                            value__ = map_.next_value()?;
                        }
                    }
                }
                Ok(MemberMap {
                    key: key__,
                    value: value__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.identity.associations.MemberMap", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Passkey {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.key.is_empty() {
            len += 1;
        }
        if self.relying_partner.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.identity.associations.Passkey", len)?;
        if !self.key.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("key", pbjson::private::base64::encode(&self.key).as_str())?;
        }
        if let Some(v) = self.relying_partner.as_ref() {
            struct_ser.serialize_field("relyingPartner", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Passkey {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "key",
            "relying_partner",
            "relyingPartner",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Key,
            RelyingPartner,
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
                            "relyingPartner" | "relying_partner" => Ok(GeneratedField::RelyingPartner),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Passkey;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.identity.associations.Passkey")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<Passkey, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut key__ = None;
                let mut relying_partner__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Key => {
                            if key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("key"));
                            }
                            key__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::RelyingPartner => {
                            if relying_partner__.is_some() {
                                return Err(serde::de::Error::duplicate_field("relyingPartner"));
                            }
                            relying_partner__ = map_.next_value()?;
                        }
                    }
                }
                Ok(Passkey {
                    key: key__.unwrap_or_default(),
                    relying_partner: relying_partner__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.identity.associations.Passkey", FIELDS, GeneratedVisitor)
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
            #[allow(clippy::needless_borrows_for_generic_args)]
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
        if !self.public_key.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.identity.associations.RecoverableEd25519Signature", len)?;
        if !self.bytes.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("bytes", pbjson::private::base64::encode(&self.bytes).as_str())?;
        }
        if !self.public_key.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("publicKey", pbjson::private::base64::encode(&self.public_key).as_str())?;
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
            "public_key",
            "publicKey",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Bytes,
            PublicKey,
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
                            "publicKey" | "public_key" => Ok(GeneratedField::PublicKey),
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
                let mut public_key__ = None;
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
                        GeneratedField::PublicKey => {
                            if public_key__.is_some() {
                                return Err(serde::de::Error::duplicate_field("publicKey"));
                            }
                            public_key__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(RecoverableEd25519Signature {
                    bytes: bytes__.unwrap_or_default(),
                    public_key: public_key__.unwrap_or_default(),
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
        if self.recovery_identifier_signature.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.identity.associations.RevokeAssociation", len)?;
        if let Some(v) = self.member_to_revoke.as_ref() {
            struct_ser.serialize_field("memberToRevoke", v)?;
        }
        if let Some(v) = self.recovery_identifier_signature.as_ref() {
            struct_ser.serialize_field("recoveryIdentifierSignature", v)?;
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
            "recovery_identifier_signature",
            "recoveryIdentifierSignature",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            MemberToRevoke,
            RecoveryIdentifierSignature,
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
                            "recoveryIdentifierSignature" | "recovery_identifier_signature" => Ok(GeneratedField::RecoveryIdentifierSignature),
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
                let mut recovery_identifier_signature__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::MemberToRevoke => {
                            if member_to_revoke__.is_some() {
                                return Err(serde::de::Error::duplicate_field("memberToRevoke"));
                            }
                            member_to_revoke__ = map_.next_value()?;
                        }
                        GeneratedField::RecoveryIdentifierSignature => {
                            if recovery_identifier_signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("recoveryIdentifierSignature"));
                            }
                            recovery_identifier_signature__ = map_.next_value()?;
                        }
                    }
                }
                Ok(RevokeAssociation {
                    member_to_revoke: member_to_revoke__,
                    recovery_identifier_signature: recovery_identifier_signature__,
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
                signature::Signature::Erc6492(v) => {
                    struct_ser.serialize_field("erc6492", v)?;
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
            "erc_6492",
            "erc6492",
            "installation_key",
            "installationKey",
            "delegated_erc_191",
            "delegatedErc191",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Erc191,
            Erc6492,
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
                            "erc6492" | "erc_6492" => Ok(GeneratedField::Erc6492),
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
                        GeneratedField::Erc6492 => {
                            if signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("erc6492"));
                            }
                            signature__ = map_.next_value::<::std::option::Option<_>>()?.map(signature::Signature::Erc6492)
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
impl serde::Serialize for SmartContractWalletSignature {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.account_id.is_empty() {
            len += 1;
        }
        if self.block_number != 0 {
            len += 1;
        }
        if !self.signature.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.identity.associations.SmartContractWalletSignature", len)?;
        if !self.account_id.is_empty() {
            struct_ser.serialize_field("accountId", &self.account_id)?;
        }
        if self.block_number != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("blockNumber", ToString::to_string(&self.block_number).as_str())?;
        }
        if !self.signature.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("signature", pbjson::private::base64::encode(&self.signature).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SmartContractWalletSignature {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "account_id",
            "accountId",
            "block_number",
            "blockNumber",
            "signature",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AccountId,
            BlockNumber,
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
                            "accountId" | "account_id" => Ok(GeneratedField::AccountId),
                            "blockNumber" | "block_number" => Ok(GeneratedField::BlockNumber),
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
            type Value = SmartContractWalletSignature;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.identity.associations.SmartContractWalletSignature")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SmartContractWalletSignature, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut account_id__ = None;
                let mut block_number__ = None;
                let mut signature__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AccountId => {
                            if account_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("accountId"));
                            }
                            account_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::BlockNumber => {
                            if block_number__.is_some() {
                                return Err(serde::de::Error::duplicate_field("blockNumber"));
                            }
                            block_number__ = 
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
                Ok(SmartContractWalletSignature {
                    account_id: account_id__.unwrap_or_default(),
                    block_number: block_number__.unwrap_or_default(),
                    signature: signature__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.identity.associations.SmartContractWalletSignature", FIELDS, GeneratedVisitor)
    }
}
