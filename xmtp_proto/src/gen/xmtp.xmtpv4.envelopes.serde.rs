impl serde::Serialize for AuthenticatedData {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.target_topic.is_empty() {
            len += 1;
        }
        if self.depends_on.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.envelopes.AuthenticatedData", len)?;
        if !self.target_topic.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("target_topic", pbjson::private::base64::encode(&self.target_topic).as_str())?;
        }
        if let Some(v) = self.depends_on.as_ref() {
            struct_ser.serialize_field("depends_on", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AuthenticatedData {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "target_topic",
            "targetTopic",
            "depends_on",
            "dependsOn",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            TargetTopic,
            DependsOn,
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
                            "targetTopic" | "target_topic" => Ok(GeneratedField::TargetTopic),
                            "dependsOn" | "depends_on" => Ok(GeneratedField::DependsOn),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AuthenticatedData;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.envelopes.AuthenticatedData")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AuthenticatedData, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut target_topic__ = None;
                let mut depends_on__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::TargetTopic => {
                            if target_topic__.is_some() {
                                return Err(serde::de::Error::duplicate_field("targetTopic"));
                            }
                            target_topic__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DependsOn => {
                            if depends_on__.is_some() {
                                return Err(serde::de::Error::duplicate_field("dependsOn"));
                            }
                            depends_on__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AuthenticatedData {
                    target_topic: target_topic__.unwrap_or_default(),
                    depends_on: depends_on__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.envelopes.AuthenticatedData", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for BlockchainProof {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.transaction_hash.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.envelopes.BlockchainProof", len)?;
        if !self.transaction_hash.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("transaction_hash", pbjson::private::base64::encode(&self.transaction_hash).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for BlockchainProof {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "transaction_hash",
            "transactionHash",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            TransactionHash,
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
                            "transactionHash" | "transaction_hash" => Ok(GeneratedField::TransactionHash),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = BlockchainProof;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.envelopes.BlockchainProof")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<BlockchainProof, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut transaction_hash__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::TransactionHash => {
                            if transaction_hash__.is_some() {
                                return Err(serde::de::Error::duplicate_field("transactionHash"));
                            }
                            transaction_hash__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(BlockchainProof {
                    transaction_hash: transaction_hash__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.envelopes.BlockchainProof", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ClientEnvelope {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.aad.is_some() {
            len += 1;
        }
        if self.payload.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.envelopes.ClientEnvelope", len)?;
        if let Some(v) = self.aad.as_ref() {
            struct_ser.serialize_field("aad", v)?;
        }
        if let Some(v) = self.payload.as_ref() {
            match v {
                client_envelope::Payload::GroupMessage(v) => {
                    struct_ser.serialize_field("group_message", v)?;
                }
                client_envelope::Payload::WelcomeMessage(v) => {
                    struct_ser.serialize_field("welcome_message", v)?;
                }
                client_envelope::Payload::UploadKeyPackage(v) => {
                    struct_ser.serialize_field("upload_key_package", v)?;
                }
                client_envelope::Payload::IdentityUpdate(v) => {
                    struct_ser.serialize_field("identity_update", v)?;
                }
                client_envelope::Payload::PayerReport(v) => {
                    struct_ser.serialize_field("payer_report", v)?;
                }
                client_envelope::Payload::PayerReportAttestation(v) => {
                    struct_ser.serialize_field("payer_report_attestation", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ClientEnvelope {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "aad",
            "group_message",
            "groupMessage",
            "welcome_message",
            "welcomeMessage",
            "upload_key_package",
            "uploadKeyPackage",
            "identity_update",
            "identityUpdate",
            "payer_report",
            "payerReport",
            "payer_report_attestation",
            "payerReportAttestation",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Aad,
            GroupMessage,
            WelcomeMessage,
            UploadKeyPackage,
            IdentityUpdate,
            PayerReport,
            PayerReportAttestation,
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
                            "aad" => Ok(GeneratedField::Aad),
                            "groupMessage" | "group_message" => Ok(GeneratedField::GroupMessage),
                            "welcomeMessage" | "welcome_message" => Ok(GeneratedField::WelcomeMessage),
                            "uploadKeyPackage" | "upload_key_package" => Ok(GeneratedField::UploadKeyPackage),
                            "identityUpdate" | "identity_update" => Ok(GeneratedField::IdentityUpdate),
                            "payerReport" | "payer_report" => Ok(GeneratedField::PayerReport),
                            "payerReportAttestation" | "payer_report_attestation" => Ok(GeneratedField::PayerReportAttestation),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ClientEnvelope;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.envelopes.ClientEnvelope")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ClientEnvelope, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut aad__ = None;
                let mut payload__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Aad => {
                            if aad__.is_some() {
                                return Err(serde::de::Error::duplicate_field("aad"));
                            }
                            aad__ = map_.next_value()?;
                        }
                        GeneratedField::GroupMessage => {
                            if payload__.is_some() {
                                return Err(serde::de::Error::duplicate_field("groupMessage"));
                            }
                            payload__ = map_.next_value::<::std::option::Option<_>>()?.map(client_envelope::Payload::GroupMessage)
;
                        }
                        GeneratedField::WelcomeMessage => {
                            if payload__.is_some() {
                                return Err(serde::de::Error::duplicate_field("welcomeMessage"));
                            }
                            payload__ = map_.next_value::<::std::option::Option<_>>()?.map(client_envelope::Payload::WelcomeMessage)
;
                        }
                        GeneratedField::UploadKeyPackage => {
                            if payload__.is_some() {
                                return Err(serde::de::Error::duplicate_field("uploadKeyPackage"));
                            }
                            payload__ = map_.next_value::<::std::option::Option<_>>()?.map(client_envelope::Payload::UploadKeyPackage)
;
                        }
                        GeneratedField::IdentityUpdate => {
                            if payload__.is_some() {
                                return Err(serde::de::Error::duplicate_field("identityUpdate"));
                            }
                            payload__ = map_.next_value::<::std::option::Option<_>>()?.map(client_envelope::Payload::IdentityUpdate)
;
                        }
                        GeneratedField::PayerReport => {
                            if payload__.is_some() {
                                return Err(serde::de::Error::duplicate_field("payerReport"));
                            }
                            payload__ = map_.next_value::<::std::option::Option<_>>()?.map(client_envelope::Payload::PayerReport)
;
                        }
                        GeneratedField::PayerReportAttestation => {
                            if payload__.is_some() {
                                return Err(serde::de::Error::duplicate_field("payerReportAttestation"));
                            }
                            payload__ = map_.next_value::<::std::option::Option<_>>()?.map(client_envelope::Payload::PayerReportAttestation)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ClientEnvelope {
                    aad: aad__,
                    payload: payload__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.envelopes.ClientEnvelope", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Cursor {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.node_id_to_sequence_id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.envelopes.Cursor", len)?;
        if !self.node_id_to_sequence_id.is_empty() {
            let v: std::collections::HashMap<_, _> = self.node_id_to_sequence_id.iter()
                .map(|(k, v)| (k, v.to_string())).collect();
            struct_ser.serialize_field("node_id_to_sequence_id", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Cursor {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "node_id_to_sequence_id",
            "nodeIdToSequenceId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            NodeIdToSequenceId,
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
                            "nodeIdToSequenceId" | "node_id_to_sequence_id" => Ok(GeneratedField::NodeIdToSequenceId),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Cursor;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.envelopes.Cursor")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<Cursor, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut node_id_to_sequence_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::NodeIdToSequenceId => {
                            if node_id_to_sequence_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nodeIdToSequenceId"));
                            }
                            node_id_to_sequence_id__ = Some(
                                map_.next_value::<std::collections::HashMap<::pbjson::private::NumberDeserialize<u32>, ::pbjson::private::NumberDeserialize<u64>>>()?
                                    .into_iter().map(|(k,v)| (k.0, v.0)).collect()
                            );
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(Cursor {
                    node_id_to_sequence_id: node_id_to_sequence_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.envelopes.Cursor", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for NodeSignature {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.node_id != 0 {
            len += 1;
        }
        if self.signature.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.envelopes.NodeSignature", len)?;
        if self.node_id != 0 {
            struct_ser.serialize_field("node_id", &self.node_id)?;
        }
        if let Some(v) = self.signature.as_ref() {
            struct_ser.serialize_field("signature", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for NodeSignature {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "node_id",
            "nodeId",
            "signature",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            NodeId,
            Signature,
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
                            "nodeId" | "node_id" => Ok(GeneratedField::NodeId),
                            "signature" => Ok(GeneratedField::Signature),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = NodeSignature;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.envelopes.NodeSignature")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<NodeSignature, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut node_id__ = None;
                let mut signature__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::NodeId => {
                            if node_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("nodeId"));
                            }
                            node_id__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Signature => {
                            if signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("signature"));
                            }
                            signature__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(NodeSignature {
                    node_id: node_id__.unwrap_or_default(),
                    signature: signature__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.envelopes.NodeSignature", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for OriginatorEnvelope {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.unsigned_originator_envelope.is_empty() {
            len += 1;
        }
        if self.proof.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.envelopes.OriginatorEnvelope", len)?;
        if !self.unsigned_originator_envelope.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("unsigned_originator_envelope", pbjson::private::base64::encode(&self.unsigned_originator_envelope).as_str())?;
        }
        if let Some(v) = self.proof.as_ref() {
            match v {
                originator_envelope::Proof::OriginatorSignature(v) => {
                    struct_ser.serialize_field("originator_signature", v)?;
                }
                originator_envelope::Proof::BlockchainProof(v) => {
                    struct_ser.serialize_field("blockchain_proof", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for OriginatorEnvelope {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "unsigned_originator_envelope",
            "unsignedOriginatorEnvelope",
            "originator_signature",
            "originatorSignature",
            "blockchain_proof",
            "blockchainProof",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            UnsignedOriginatorEnvelope,
            OriginatorSignature,
            BlockchainProof,
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
                            "unsignedOriginatorEnvelope" | "unsigned_originator_envelope" => Ok(GeneratedField::UnsignedOriginatorEnvelope),
                            "originatorSignature" | "originator_signature" => Ok(GeneratedField::OriginatorSignature),
                            "blockchainProof" | "blockchain_proof" => Ok(GeneratedField::BlockchainProof),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = OriginatorEnvelope;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.envelopes.OriginatorEnvelope")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<OriginatorEnvelope, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut unsigned_originator_envelope__ = None;
                let mut proof__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::UnsignedOriginatorEnvelope => {
                            if unsigned_originator_envelope__.is_some() {
                                return Err(serde::de::Error::duplicate_field("unsignedOriginatorEnvelope"));
                            }
                            unsigned_originator_envelope__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::OriginatorSignature => {
                            if proof__.is_some() {
                                return Err(serde::de::Error::duplicate_field("originatorSignature"));
                            }
                            proof__ = map_.next_value::<::std::option::Option<_>>()?.map(originator_envelope::Proof::OriginatorSignature)
;
                        }
                        GeneratedField::BlockchainProof => {
                            if proof__.is_some() {
                                return Err(serde::de::Error::duplicate_field("blockchainProof"));
                            }
                            proof__ = map_.next_value::<::std::option::Option<_>>()?.map(originator_envelope::Proof::BlockchainProof)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(OriginatorEnvelope {
                    unsigned_originator_envelope: unsigned_originator_envelope__.unwrap_or_default(),
                    proof: proof__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.envelopes.OriginatorEnvelope", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for PayerEnvelope {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.unsigned_client_envelope.is_empty() {
            len += 1;
        }
        if self.payer_signature.is_some() {
            len += 1;
        }
        if self.target_originator != 0 {
            len += 1;
        }
        if self.message_retention_days != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.envelopes.PayerEnvelope", len)?;
        if !self.unsigned_client_envelope.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("unsigned_client_envelope", pbjson::private::base64::encode(&self.unsigned_client_envelope).as_str())?;
        }
        if let Some(v) = self.payer_signature.as_ref() {
            struct_ser.serialize_field("payer_signature", v)?;
        }
        if self.target_originator != 0 {
            struct_ser.serialize_field("target_originator", &self.target_originator)?;
        }
        if self.message_retention_days != 0 {
            struct_ser.serialize_field("message_retention_days", &self.message_retention_days)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for PayerEnvelope {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "unsigned_client_envelope",
            "unsignedClientEnvelope",
            "payer_signature",
            "payerSignature",
            "target_originator",
            "targetOriginator",
            "message_retention_days",
            "messageRetentionDays",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            UnsignedClientEnvelope,
            PayerSignature,
            TargetOriginator,
            MessageRetentionDays,
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
                            "unsignedClientEnvelope" | "unsigned_client_envelope" => Ok(GeneratedField::UnsignedClientEnvelope),
                            "payerSignature" | "payer_signature" => Ok(GeneratedField::PayerSignature),
                            "targetOriginator" | "target_originator" => Ok(GeneratedField::TargetOriginator),
                            "messageRetentionDays" | "message_retention_days" => Ok(GeneratedField::MessageRetentionDays),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = PayerEnvelope;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.envelopes.PayerEnvelope")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<PayerEnvelope, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut unsigned_client_envelope__ = None;
                let mut payer_signature__ = None;
                let mut target_originator__ = None;
                let mut message_retention_days__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::UnsignedClientEnvelope => {
                            if unsigned_client_envelope__.is_some() {
                                return Err(serde::de::Error::duplicate_field("unsignedClientEnvelope"));
                            }
                            unsigned_client_envelope__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::PayerSignature => {
                            if payer_signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("payerSignature"));
                            }
                            payer_signature__ = map_.next_value()?;
                        }
                        GeneratedField::TargetOriginator => {
                            if target_originator__.is_some() {
                                return Err(serde::de::Error::duplicate_field("targetOriginator"));
                            }
                            target_originator__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::MessageRetentionDays => {
                            if message_retention_days__.is_some() {
                                return Err(serde::de::Error::duplicate_field("messageRetentionDays"));
                            }
                            message_retention_days__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(PayerEnvelope {
                    unsigned_client_envelope: unsigned_client_envelope__.unwrap_or_default(),
                    payer_signature: payer_signature__,
                    target_originator: target_originator__.unwrap_or_default(),
                    message_retention_days: message_retention_days__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.envelopes.PayerEnvelope", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for PayerReport {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.originator_node_id != 0 {
            len += 1;
        }
        if self.start_sequence_id != 0 {
            len += 1;
        }
        if self.end_sequence_id != 0 {
            len += 1;
        }
        if self.end_minute_since_epoch != 0 {
            len += 1;
        }
        if !self.payers_merkle_root.is_empty() {
            len += 1;
        }
        if !self.active_node_ids.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.envelopes.PayerReport", len)?;
        if self.originator_node_id != 0 {
            struct_ser.serialize_field("originator_node_id", &self.originator_node_id)?;
        }
        if self.start_sequence_id != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("start_sequence_id", ToString::to_string(&self.start_sequence_id).as_str())?;
        }
        if self.end_sequence_id != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("end_sequence_id", ToString::to_string(&self.end_sequence_id).as_str())?;
        }
        if self.end_minute_since_epoch != 0 {
            struct_ser.serialize_field("end_minute_since_epoch", &self.end_minute_since_epoch)?;
        }
        if !self.payers_merkle_root.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("payers_merkle_root", pbjson::private::base64::encode(&self.payers_merkle_root).as_str())?;
        }
        if !self.active_node_ids.is_empty() {
            struct_ser.serialize_field("active_node_ids", &self.active_node_ids)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for PayerReport {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "originator_node_id",
            "originatorNodeId",
            "start_sequence_id",
            "startSequenceId",
            "end_sequence_id",
            "endSequenceId",
            "end_minute_since_epoch",
            "endMinuteSinceEpoch",
            "payers_merkle_root",
            "payersMerkleRoot",
            "active_node_ids",
            "activeNodeIds",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            OriginatorNodeId,
            StartSequenceId,
            EndSequenceId,
            EndMinuteSinceEpoch,
            PayersMerkleRoot,
            ActiveNodeIds,
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
                            "originatorNodeId" | "originator_node_id" => Ok(GeneratedField::OriginatorNodeId),
                            "startSequenceId" | "start_sequence_id" => Ok(GeneratedField::StartSequenceId),
                            "endSequenceId" | "end_sequence_id" => Ok(GeneratedField::EndSequenceId),
                            "endMinuteSinceEpoch" | "end_minute_since_epoch" => Ok(GeneratedField::EndMinuteSinceEpoch),
                            "payersMerkleRoot" | "payers_merkle_root" => Ok(GeneratedField::PayersMerkleRoot),
                            "activeNodeIds" | "active_node_ids" => Ok(GeneratedField::ActiveNodeIds),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = PayerReport;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.envelopes.PayerReport")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<PayerReport, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut originator_node_id__ = None;
                let mut start_sequence_id__ = None;
                let mut end_sequence_id__ = None;
                let mut end_minute_since_epoch__ = None;
                let mut payers_merkle_root__ = None;
                let mut active_node_ids__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::OriginatorNodeId => {
                            if originator_node_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("originatorNodeId"));
                            }
                            originator_node_id__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::StartSequenceId => {
                            if start_sequence_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("startSequenceId"));
                            }
                            start_sequence_id__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::EndSequenceId => {
                            if end_sequence_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("endSequenceId"));
                            }
                            end_sequence_id__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::EndMinuteSinceEpoch => {
                            if end_minute_since_epoch__.is_some() {
                                return Err(serde::de::Error::duplicate_field("endMinuteSinceEpoch"));
                            }
                            end_minute_since_epoch__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::PayersMerkleRoot => {
                            if payers_merkle_root__.is_some() {
                                return Err(serde::de::Error::duplicate_field("payersMerkleRoot"));
                            }
                            payers_merkle_root__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ActiveNodeIds => {
                            if active_node_ids__.is_some() {
                                return Err(serde::de::Error::duplicate_field("activeNodeIds"));
                            }
                            active_node_ids__ = 
                                Some(map_.next_value::<Vec<::pbjson::private::NumberDeserialize<_>>>()?
                                    .into_iter().map(|x| x.0).collect())
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(PayerReport {
                    originator_node_id: originator_node_id__.unwrap_or_default(),
                    start_sequence_id: start_sequence_id__.unwrap_or_default(),
                    end_sequence_id: end_sequence_id__.unwrap_or_default(),
                    end_minute_since_epoch: end_minute_since_epoch__.unwrap_or_default(),
                    payers_merkle_root: payers_merkle_root__.unwrap_or_default(),
                    active_node_ids: active_node_ids__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.envelopes.PayerReport", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for PayerReportAttestation {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.report_id.is_empty() {
            len += 1;
        }
        if self.signature.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.envelopes.PayerReportAttestation", len)?;
        if !self.report_id.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("report_id", pbjson::private::base64::encode(&self.report_id).as_str())?;
        }
        if let Some(v) = self.signature.as_ref() {
            struct_ser.serialize_field("signature", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for PayerReportAttestation {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "report_id",
            "reportId",
            "signature",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ReportId,
            Signature,
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
                            "reportId" | "report_id" => Ok(GeneratedField::ReportId),
                            "signature" => Ok(GeneratedField::Signature),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = PayerReportAttestation;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.envelopes.PayerReportAttestation")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<PayerReportAttestation, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut report_id__ = None;
                let mut signature__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ReportId => {
                            if report_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("reportId"));
                            }
                            report_id__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Signature => {
                            if signature__.is_some() {
                                return Err(serde::de::Error::duplicate_field("signature"));
                            }
                            signature__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(PayerReportAttestation {
                    report_id: report_id__.unwrap_or_default(),
                    signature: signature__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.envelopes.PayerReportAttestation", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for UnsignedOriginatorEnvelope {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.originator_node_id != 0 {
            len += 1;
        }
        if self.originator_sequence_id != 0 {
            len += 1;
        }
        if self.originator_ns != 0 {
            len += 1;
        }
        if !self.payer_envelope_bytes.is_empty() {
            len += 1;
        }
        if self.base_fee_picodollars != 0 {
            len += 1;
        }
        if self.congestion_fee_picodollars != 0 {
            len += 1;
        }
        if self.expiry_unixtime != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.envelopes.UnsignedOriginatorEnvelope", len)?;
        if self.originator_node_id != 0 {
            struct_ser.serialize_field("originator_node_id", &self.originator_node_id)?;
        }
        if self.originator_sequence_id != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("originator_sequence_id", ToString::to_string(&self.originator_sequence_id).as_str())?;
        }
        if self.originator_ns != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("originator_ns", ToString::to_string(&self.originator_ns).as_str())?;
        }
        if !self.payer_envelope_bytes.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("payer_envelope_bytes", pbjson::private::base64::encode(&self.payer_envelope_bytes).as_str())?;
        }
        if self.base_fee_picodollars != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("base_fee_picodollars", ToString::to_string(&self.base_fee_picodollars).as_str())?;
        }
        if self.congestion_fee_picodollars != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("congestion_fee_picodollars", ToString::to_string(&self.congestion_fee_picodollars).as_str())?;
        }
        if self.expiry_unixtime != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("expiry_unixtime", ToString::to_string(&self.expiry_unixtime).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for UnsignedOriginatorEnvelope {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "originator_node_id",
            "originatorNodeId",
            "originator_sequence_id",
            "originatorSequenceId",
            "originator_ns",
            "originatorNs",
            "payer_envelope_bytes",
            "payerEnvelopeBytes",
            "base_fee_picodollars",
            "baseFeePicodollars",
            "congestion_fee_picodollars",
            "congestionFeePicodollars",
            "expiry_unixtime",
            "expiryUnixtime",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            OriginatorNodeId,
            OriginatorSequenceId,
            OriginatorNs,
            PayerEnvelopeBytes,
            BaseFeePicodollars,
            CongestionFeePicodollars,
            ExpiryUnixtime,
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
                            "originatorNodeId" | "originator_node_id" => Ok(GeneratedField::OriginatorNodeId),
                            "originatorSequenceId" | "originator_sequence_id" => Ok(GeneratedField::OriginatorSequenceId),
                            "originatorNs" | "originator_ns" => Ok(GeneratedField::OriginatorNs),
                            "payerEnvelopeBytes" | "payer_envelope_bytes" => Ok(GeneratedField::PayerEnvelopeBytes),
                            "baseFeePicodollars" | "base_fee_picodollars" => Ok(GeneratedField::BaseFeePicodollars),
                            "congestionFeePicodollars" | "congestion_fee_picodollars" => Ok(GeneratedField::CongestionFeePicodollars),
                            "expiryUnixtime" | "expiry_unixtime" => Ok(GeneratedField::ExpiryUnixtime),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = UnsignedOriginatorEnvelope;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.envelopes.UnsignedOriginatorEnvelope")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<UnsignedOriginatorEnvelope, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut originator_node_id__ = None;
                let mut originator_sequence_id__ = None;
                let mut originator_ns__ = None;
                let mut payer_envelope_bytes__ = None;
                let mut base_fee_picodollars__ = None;
                let mut congestion_fee_picodollars__ = None;
                let mut expiry_unixtime__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::OriginatorNodeId => {
                            if originator_node_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("originatorNodeId"));
                            }
                            originator_node_id__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::OriginatorSequenceId => {
                            if originator_sequence_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("originatorSequenceId"));
                            }
                            originator_sequence_id__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::OriginatorNs => {
                            if originator_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("originatorNs"));
                            }
                            originator_ns__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::PayerEnvelopeBytes => {
                            if payer_envelope_bytes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("payerEnvelopeBytes"));
                            }
                            payer_envelope_bytes__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::BaseFeePicodollars => {
                            if base_fee_picodollars__.is_some() {
                                return Err(serde::de::Error::duplicate_field("baseFeePicodollars"));
                            }
                            base_fee_picodollars__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::CongestionFeePicodollars => {
                            if congestion_fee_picodollars__.is_some() {
                                return Err(serde::de::Error::duplicate_field("congestionFeePicodollars"));
                            }
                            congestion_fee_picodollars__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ExpiryUnixtime => {
                            if expiry_unixtime__.is_some() {
                                return Err(serde::de::Error::duplicate_field("expiryUnixtime"));
                            }
                            expiry_unixtime__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(UnsignedOriginatorEnvelope {
                    originator_node_id: originator_node_id__.unwrap_or_default(),
                    originator_sequence_id: originator_sequence_id__.unwrap_or_default(),
                    originator_ns: originator_ns__.unwrap_or_default(),
                    payer_envelope_bytes: payer_envelope_bytes__.unwrap_or_default(),
                    base_fee_picodollars: base_fee_picodollars__.unwrap_or_default(),
                    congestion_fee_picodollars: congestion_fee_picodollars__.unwrap_or_default(),
                    expiry_unixtime: expiry_unixtime__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.envelopes.UnsignedOriginatorEnvelope", FIELDS, GeneratedVisitor)
    }
}
