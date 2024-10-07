// @generated
impl serde::Serialize for AuthenticatedData {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.target_originator != 0 {
            len += 1;
        }
        if !self.target_topic.is_empty() {
            len += 1;
        }
        if self.last_seen.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.AuthenticatedData", len)?;
        if self.target_originator != 0 {
            struct_ser.serialize_field("targetOriginator", &self.target_originator)?;
        }
        if !self.target_topic.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("targetTopic", pbjson::private::base64::encode(&self.target_topic).as_str())?;
        }
        if let Some(v) = self.last_seen.as_ref() {
            struct_ser.serialize_field("lastSeen", v)?;
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
            "target_originator",
            "targetOriginator",
            "target_topic",
            "targetTopic",
            "last_seen",
            "lastSeen",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            TargetOriginator,
            TargetTopic,
            LastSeen,
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
                            "targetOriginator" | "target_originator" => Ok(GeneratedField::TargetOriginator),
                            "targetTopic" | "target_topic" => Ok(GeneratedField::TargetTopic),
                            "lastSeen" | "last_seen" => Ok(GeneratedField::LastSeen),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
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
                formatter.write_str("struct xmtp.xmtpv4.AuthenticatedData")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AuthenticatedData, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut target_originator__ = None;
                let mut target_topic__ = None;
                let mut last_seen__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::TargetOriginator => {
                            if target_originator__.is_some() {
                                return Err(serde::de::Error::duplicate_field("targetOriginator"));
                            }
                            target_originator__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::TargetTopic => {
                            if target_topic__.is_some() {
                                return Err(serde::de::Error::duplicate_field("targetTopic"));
                            }
                            target_topic__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::LastSeen => {
                            if last_seen__.is_some() {
                                return Err(serde::de::Error::duplicate_field("lastSeen"));
                            }
                            last_seen__ = map_.next_value()?;
                        }
                    }
                }
                Ok(AuthenticatedData {
                    target_originator: target_originator__.unwrap_or_default(),
                    target_topic: target_topic__.unwrap_or_default(),
                    last_seen: last_seen__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.AuthenticatedData", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for BatchSubscribeEnvelopesRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.requests.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.BatchSubscribeEnvelopesRequest", len)?;
        if !self.requests.is_empty() {
            struct_ser.serialize_field("requests", &self.requests)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for BatchSubscribeEnvelopesRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "requests",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Requests,
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
                            "requests" => Ok(GeneratedField::Requests),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = BatchSubscribeEnvelopesRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.BatchSubscribeEnvelopesRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<BatchSubscribeEnvelopesRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut requests__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Requests => {
                            if requests__.is_some() {
                                return Err(serde::de::Error::duplicate_field("requests"));
                            }
                            requests__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(BatchSubscribeEnvelopesRequest {
                    requests: requests__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.BatchSubscribeEnvelopesRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for batch_subscribe_envelopes_request::SubscribeEnvelopesRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.query.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.BatchSubscribeEnvelopesRequest.SubscribeEnvelopesRequest", len)?;
        if let Some(v) = self.query.as_ref() {
            struct_ser.serialize_field("query", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for batch_subscribe_envelopes_request::SubscribeEnvelopesRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "query",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Query,
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
                            "query" => Ok(GeneratedField::Query),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = batch_subscribe_envelopes_request::SubscribeEnvelopesRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.BatchSubscribeEnvelopesRequest.SubscribeEnvelopesRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<batch_subscribe_envelopes_request::SubscribeEnvelopesRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut query__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Query => {
                            if query__.is_some() {
                                return Err(serde::de::Error::duplicate_field("query"));
                            }
                            query__ = map_.next_value()?;
                        }
                    }
                }
                Ok(batch_subscribe_envelopes_request::SubscribeEnvelopesRequest {
                    query: query__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.BatchSubscribeEnvelopesRequest.SubscribeEnvelopesRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for BatchSubscribeEnvelopesResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.envelopes.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.BatchSubscribeEnvelopesResponse", len)?;
        if !self.envelopes.is_empty() {
            struct_ser.serialize_field("envelopes", &self.envelopes)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for BatchSubscribeEnvelopesResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "envelopes",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Envelopes,
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
                            "envelopes" => Ok(GeneratedField::Envelopes),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = BatchSubscribeEnvelopesResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.BatchSubscribeEnvelopesResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<BatchSubscribeEnvelopesResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut envelopes__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Envelopes => {
                            if envelopes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("envelopes"));
                            }
                            envelopes__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(BatchSubscribeEnvelopesResponse {
                    envelopes: envelopes__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.BatchSubscribeEnvelopesResponse", FIELDS, GeneratedVisitor)
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
        if self.block_number != 0 {
            len += 1;
        }
        if self.publisher_node_id != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.BlockchainProof", len)?;
        if self.block_number != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("blockNumber", ToString::to_string(&self.block_number).as_str())?;
        }
        if self.publisher_node_id != 0 {
            struct_ser.serialize_field("publisherNodeId", &self.publisher_node_id)?;
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
            "block_number",
            "blockNumber",
            "publisher_node_id",
            "publisherNodeId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            BlockNumber,
            PublisherNodeId,
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
                            "blockNumber" | "block_number" => Ok(GeneratedField::BlockNumber),
                            "publisherNodeId" | "publisher_node_id" => Ok(GeneratedField::PublisherNodeId),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
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
                formatter.write_str("struct xmtp.xmtpv4.BlockchainProof")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<BlockchainProof, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut block_number__ = None;
                let mut publisher_node_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::BlockNumber => {
                            if block_number__.is_some() {
                                return Err(serde::de::Error::duplicate_field("blockNumber"));
                            }
                            block_number__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::PublisherNodeId => {
                            if publisher_node_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("publisherNodeId"));
                            }
                            publisher_node_id__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(BlockchainProof {
                    block_number: block_number__.unwrap_or_default(),
                    publisher_node_id: publisher_node_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.BlockchainProof", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.ClientEnvelope", len)?;
        if let Some(v) = self.aad.as_ref() {
            struct_ser.serialize_field("aad", v)?;
        }
        if let Some(v) = self.payload.as_ref() {
            match v {
                client_envelope::Payload::GroupMessage(v) => {
                    struct_ser.serialize_field("groupMessage", v)?;
                }
                client_envelope::Payload::WelcomeMessage(v) => {
                    struct_ser.serialize_field("welcomeMessage", v)?;
                }
                client_envelope::Payload::RegisterInstallation(v) => {
                    struct_ser.serialize_field("registerInstallation", v)?;
                }
                client_envelope::Payload::UploadKeyPackage(v) => {
                    struct_ser.serialize_field("uploadKeyPackage", v)?;
                }
                client_envelope::Payload::RevokeInstallation(v) => {
                    struct_ser.serialize_field("revokeInstallation", v)?;
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
            "register_installation",
            "registerInstallation",
            "upload_key_package",
            "uploadKeyPackage",
            "revoke_installation",
            "revokeInstallation",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Aad,
            GroupMessage,
            WelcomeMessage,
            RegisterInstallation,
            UploadKeyPackage,
            RevokeInstallation,
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
                            "registerInstallation" | "register_installation" => Ok(GeneratedField::RegisterInstallation),
                            "uploadKeyPackage" | "upload_key_package" => Ok(GeneratedField::UploadKeyPackage),
                            "revokeInstallation" | "revoke_installation" => Ok(GeneratedField::RevokeInstallation),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
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
                formatter.write_str("struct xmtp.xmtpv4.ClientEnvelope")
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
                        GeneratedField::RegisterInstallation => {
                            if payload__.is_some() {
                                return Err(serde::de::Error::duplicate_field("registerInstallation"));
                            }
                            payload__ = map_.next_value::<::std::option::Option<_>>()?.map(client_envelope::Payload::RegisterInstallation)
;
                        }
                        GeneratedField::UploadKeyPackage => {
                            if payload__.is_some() {
                                return Err(serde::de::Error::duplicate_field("uploadKeyPackage"));
                            }
                            payload__ = map_.next_value::<::std::option::Option<_>>()?.map(client_envelope::Payload::UploadKeyPackage)
;
                        }
                        GeneratedField::RevokeInstallation => {
                            if payload__.is_some() {
                                return Err(serde::de::Error::duplicate_field("revokeInstallation"));
                            }
                            payload__ = map_.next_value::<::std::option::Option<_>>()?.map(client_envelope::Payload::RevokeInstallation)
;
                        }
                    }
                }
                Ok(ClientEnvelope {
                    aad: aad__,
                    payload: payload__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.ClientEnvelope", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EnvelopesQuery {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.last_seen.is_some() {
            len += 1;
        }
        if self.filter.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.EnvelopesQuery", len)?;
        if let Some(v) = self.last_seen.as_ref() {
            struct_ser.serialize_field("lastSeen", v)?;
        }
        if let Some(v) = self.filter.as_ref() {
            match v {
                envelopes_query::Filter::Topic(v) => {
                    #[allow(clippy::needless_borrow)]
                    #[allow(clippy::needless_borrows_for_generic_args)]
                    struct_ser.serialize_field("topic", pbjson::private::base64::encode(&v).as_str())?;
                }
                envelopes_query::Filter::OriginatorNodeId(v) => {
                    struct_ser.serialize_field("originatorNodeId", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EnvelopesQuery {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "last_seen",
            "lastSeen",
            "topic",
            "originator_node_id",
            "originatorNodeId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            LastSeen,
            Topic,
            OriginatorNodeId,
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
                            "lastSeen" | "last_seen" => Ok(GeneratedField::LastSeen),
                            "topic" => Ok(GeneratedField::Topic),
                            "originatorNodeId" | "originator_node_id" => Ok(GeneratedField::OriginatorNodeId),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EnvelopesQuery;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.EnvelopesQuery")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EnvelopesQuery, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut last_seen__ = None;
                let mut filter__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::LastSeen => {
                            if last_seen__.is_some() {
                                return Err(serde::de::Error::duplicate_field("lastSeen"));
                            }
                            last_seen__ = map_.next_value()?;
                        }
                        GeneratedField::Topic => {
                            if filter__.is_some() {
                                return Err(serde::de::Error::duplicate_field("topic"));
                            }
                            filter__ = map_.next_value::<::std::option::Option<::pbjson::private::BytesDeserialize<_>>>()?.map(|x| envelopes_query::Filter::Topic(x.0));
                        }
                        GeneratedField::OriginatorNodeId => {
                            if filter__.is_some() {
                                return Err(serde::de::Error::duplicate_field("originatorNodeId"));
                            }
                            filter__ = map_.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| envelopes_query::Filter::OriginatorNodeId(x.0));
                        }
                    }
                }
                Ok(EnvelopesQuery {
                    last_seen: last_seen__,
                    filter: filter__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.EnvelopesQuery", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GetInboxIdsRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.requests.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.GetInboxIdsRequest", len)?;
        if !self.requests.is_empty() {
            struct_ser.serialize_field("requests", &self.requests)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GetInboxIdsRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "requests",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Requests,
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
                            "requests" => Ok(GeneratedField::Requests),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GetInboxIdsRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.GetInboxIdsRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GetInboxIdsRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut requests__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Requests => {
                            if requests__.is_some() {
                                return Err(serde::de::Error::duplicate_field("requests"));
                            }
                            requests__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(GetInboxIdsRequest {
                    requests: requests__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.GetInboxIdsRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for get_inbox_ids_request::Request {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.address.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.GetInboxIdsRequest.Request", len)?;
        if !self.address.is_empty() {
            struct_ser.serialize_field("address", &self.address)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for get_inbox_ids_request::Request {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "address",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Address,
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
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = get_inbox_ids_request::Request;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.GetInboxIdsRequest.Request")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<get_inbox_ids_request::Request, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut address__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Address => {
                            if address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("address"));
                            }
                            address__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(get_inbox_ids_request::Request {
                    address: address__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.GetInboxIdsRequest.Request", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GetInboxIdsResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.responses.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.GetInboxIdsResponse", len)?;
        if !self.responses.is_empty() {
            struct_ser.serialize_field("responses", &self.responses)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GetInboxIdsResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "responses",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Responses,
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
                            "responses" => Ok(GeneratedField::Responses),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GetInboxIdsResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.GetInboxIdsResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GetInboxIdsResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut responses__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Responses => {
                            if responses__.is_some() {
                                return Err(serde::de::Error::duplicate_field("responses"));
                            }
                            responses__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(GetInboxIdsResponse {
                    responses: responses__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.GetInboxIdsResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for get_inbox_ids_response::Response {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.address.is_empty() {
            len += 1;
        }
        if self.inbox_id.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.GetInboxIdsResponse.Response", len)?;
        if !self.address.is_empty() {
            struct_ser.serialize_field("address", &self.address)?;
        }
        if let Some(v) = self.inbox_id.as_ref() {
            struct_ser.serialize_field("inboxId", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for get_inbox_ids_response::Response {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "address",
            "inbox_id",
            "inboxId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Address,
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
                            "address" => Ok(GeneratedField::Address),
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
            type Value = get_inbox_ids_response::Response;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.GetInboxIdsResponse.Response")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<get_inbox_ids_response::Response, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut address__ = None;
                let mut inbox_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Address => {
                            if address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("address"));
                            }
                            address__ = Some(map_.next_value()?);
                        }
                        GeneratedField::InboxId => {
                            if inbox_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inboxId"));
                            }
                            inbox_id__ = map_.next_value()?;
                        }
                    }
                }
                Ok(get_inbox_ids_response::Response {
                    address: address__.unwrap_or_default(),
                    inbox_id: inbox_id__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.GetInboxIdsResponse.Response", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for Misbehavior {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "MISBEHAVIOR_UNSPECIFIED",
            Self::UnavailableNode => "MISBEHAVIOR_UNAVAILABLE_NODE",
            Self::OutOfOrderOriginatorSid => "MISBEHAVIOR_OUT_OF_ORDER_ORIGINATOR_SID",
            Self::DuplicateOriginatorSid => "MISBEHAVIOR_DUPLICATE_ORIGINATOR_SID",
            Self::CyclicalMessageOrdering => "MISBEHAVIOR_CYCLICAL_MESSAGE_ORDERING",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for Misbehavior {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "MISBEHAVIOR_UNSPECIFIED",
            "MISBEHAVIOR_UNAVAILABLE_NODE",
            "MISBEHAVIOR_OUT_OF_ORDER_ORIGINATOR_SID",
            "MISBEHAVIOR_DUPLICATE_ORIGINATOR_SID",
            "MISBEHAVIOR_CYCLICAL_MESSAGE_ORDERING",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Misbehavior;

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
                    "MISBEHAVIOR_UNSPECIFIED" => Ok(Misbehavior::Unspecified),
                    "MISBEHAVIOR_UNAVAILABLE_NODE" => Ok(Misbehavior::UnavailableNode),
                    "MISBEHAVIOR_OUT_OF_ORDER_ORIGINATOR_SID" => Ok(Misbehavior::OutOfOrderOriginatorSid),
                    "MISBEHAVIOR_DUPLICATE_ORIGINATOR_SID" => Ok(Misbehavior::DuplicateOriginatorSid),
                    "MISBEHAVIOR_CYCLICAL_MESSAGE_ORDERING" => Ok(Misbehavior::CyclicalMessageOrdering),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for MisbehaviorReport {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.r#type != 0 {
            len += 1;
        }
        if !self.envelopes.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.MisbehaviorReport", len)?;
        if self.r#type != 0 {
            let v = Misbehavior::try_from(self.r#type)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.r#type)))?;
            struct_ser.serialize_field("type", &v)?;
        }
        if !self.envelopes.is_empty() {
            struct_ser.serialize_field("envelopes", &self.envelopes)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for MisbehaviorReport {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "type",
            "envelopes",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Type,
            Envelopes,
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
                            "envelopes" => Ok(GeneratedField::Envelopes),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = MisbehaviorReport;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.MisbehaviorReport")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<MisbehaviorReport, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut r#type__ = None;
                let mut envelopes__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Type => {
                            if r#type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("type"));
                            }
                            r#type__ = Some(map_.next_value::<Misbehavior>()? as i32);
                        }
                        GeneratedField::Envelopes => {
                            if envelopes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("envelopes"));
                            }
                            envelopes__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(MisbehaviorReport {
                    r#type: r#type__.unwrap_or_default(),
                    envelopes: envelopes__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.MisbehaviorReport", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.OriginatorEnvelope", len)?;
        if !self.unsigned_originator_envelope.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("unsignedOriginatorEnvelope", pbjson::private::base64::encode(&self.unsigned_originator_envelope).as_str())?;
        }
        if let Some(v) = self.proof.as_ref() {
            match v {
                originator_envelope::Proof::OriginatorSignature(v) => {
                    struct_ser.serialize_field("originatorSignature", v)?;
                }
                originator_envelope::Proof::BlockchainProof(v) => {
                    struct_ser.serialize_field("blockchainProof", v)?;
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
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
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
                formatter.write_str("struct xmtp.xmtpv4.OriginatorEnvelope")
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
                    }
                }
                Ok(OriginatorEnvelope {
                    unsigned_originator_envelope: unsigned_originator_envelope__.unwrap_or_default(),
                    proof: proof__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.OriginatorEnvelope", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.PayerEnvelope", len)?;
        if !self.unsigned_client_envelope.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("unsignedClientEnvelope", pbjson::private::base64::encode(&self.unsigned_client_envelope).as_str())?;
        }
        if let Some(v) = self.payer_signature.as_ref() {
            struct_ser.serialize_field("payerSignature", v)?;
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
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            UnsignedClientEnvelope,
            PayerSignature,
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
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
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
                formatter.write_str("struct xmtp.xmtpv4.PayerEnvelope")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<PayerEnvelope, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut unsigned_client_envelope__ = None;
                let mut payer_signature__ = None;
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
                    }
                }
                Ok(PayerEnvelope {
                    unsigned_client_envelope: unsigned_client_envelope__.unwrap_or_default(),
                    payer_signature: payer_signature__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.PayerEnvelope", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for PublishEnvelopeRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.payer_envelope.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.PublishEnvelopeRequest", len)?;
        if let Some(v) = self.payer_envelope.as_ref() {
            struct_ser.serialize_field("payerEnvelope", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for PublishEnvelopeRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "payer_envelope",
            "payerEnvelope",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            PayerEnvelope,
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
                            "payerEnvelope" | "payer_envelope" => Ok(GeneratedField::PayerEnvelope),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = PublishEnvelopeRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.PublishEnvelopeRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<PublishEnvelopeRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut payer_envelope__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::PayerEnvelope => {
                            if payer_envelope__.is_some() {
                                return Err(serde::de::Error::duplicate_field("payerEnvelope"));
                            }
                            payer_envelope__ = map_.next_value()?;
                        }
                    }
                }
                Ok(PublishEnvelopeRequest {
                    payer_envelope: payer_envelope__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.PublishEnvelopeRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for PublishEnvelopeResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.originator_envelope.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.PublishEnvelopeResponse", len)?;
        if let Some(v) = self.originator_envelope.as_ref() {
            struct_ser.serialize_field("originatorEnvelope", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for PublishEnvelopeResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "originator_envelope",
            "originatorEnvelope",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            OriginatorEnvelope,
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
                            "originatorEnvelope" | "originator_envelope" => Ok(GeneratedField::OriginatorEnvelope),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = PublishEnvelopeResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.PublishEnvelopeResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<PublishEnvelopeResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut originator_envelope__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::OriginatorEnvelope => {
                            if originator_envelope__.is_some() {
                                return Err(serde::de::Error::duplicate_field("originatorEnvelope"));
                            }
                            originator_envelope__ = map_.next_value()?;
                        }
                    }
                }
                Ok(PublishEnvelopeResponse {
                    originator_envelope: originator_envelope__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.PublishEnvelopeResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for QueryEnvelopesRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.query.is_some() {
            len += 1;
        }
        if self.limit != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.QueryEnvelopesRequest", len)?;
        if let Some(v) = self.query.as_ref() {
            struct_ser.serialize_field("query", v)?;
        }
        if self.limit != 0 {
            struct_ser.serialize_field("limit", &self.limit)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for QueryEnvelopesRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "query",
            "limit",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Query,
            Limit,
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
                            "query" => Ok(GeneratedField::Query),
                            "limit" => Ok(GeneratedField::Limit),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = QueryEnvelopesRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.QueryEnvelopesRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<QueryEnvelopesRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut query__ = None;
                let mut limit__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Query => {
                            if query__.is_some() {
                                return Err(serde::de::Error::duplicate_field("query"));
                            }
                            query__ = map_.next_value()?;
                        }
                        GeneratedField::Limit => {
                            if limit__.is_some() {
                                return Err(serde::de::Error::duplicate_field("limit"));
                            }
                            limit__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(QueryEnvelopesRequest {
                    query: query__,
                    limit: limit__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.QueryEnvelopesRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for QueryEnvelopesResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.envelopes.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.QueryEnvelopesResponse", len)?;
        if !self.envelopes.is_empty() {
            struct_ser.serialize_field("envelopes", &self.envelopes)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for QueryEnvelopesResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "envelopes",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Envelopes,
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
                            "envelopes" => Ok(GeneratedField::Envelopes),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = QueryEnvelopesResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.QueryEnvelopesResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<QueryEnvelopesResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut envelopes__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Envelopes => {
                            if envelopes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("envelopes"));
                            }
                            envelopes__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(QueryEnvelopesResponse {
                    envelopes: envelopes__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.QueryEnvelopesResponse", FIELDS, GeneratedVisitor)
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
        if self.payer_envelope.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.UnsignedOriginatorEnvelope", len)?;
        if self.originator_node_id != 0 {
            struct_ser.serialize_field("originatorNodeId", &self.originator_node_id)?;
        }
        if self.originator_sequence_id != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("originatorSequenceId", ToString::to_string(&self.originator_sequence_id).as_str())?;
        }
        if self.originator_ns != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("originatorNs", ToString::to_string(&self.originator_ns).as_str())?;
        }
        if let Some(v) = self.payer_envelope.as_ref() {
            struct_ser.serialize_field("payerEnvelope", v)?;
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
            "payer_envelope",
            "payerEnvelope",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            OriginatorNodeId,
            OriginatorSequenceId,
            OriginatorNs,
            PayerEnvelope,
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
                            "payerEnvelope" | "payer_envelope" => Ok(GeneratedField::PayerEnvelope),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
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
                formatter.write_str("struct xmtp.xmtpv4.UnsignedOriginatorEnvelope")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<UnsignedOriginatorEnvelope, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut originator_node_id__ = None;
                let mut originator_sequence_id__ = None;
                let mut originator_ns__ = None;
                let mut payer_envelope__ = None;
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
                        GeneratedField::PayerEnvelope => {
                            if payer_envelope__.is_some() {
                                return Err(serde::de::Error::duplicate_field("payerEnvelope"));
                            }
                            payer_envelope__ = map_.next_value()?;
                        }
                    }
                }
                Ok(UnsignedOriginatorEnvelope {
                    originator_node_id: originator_node_id__.unwrap_or_default(),
                    originator_sequence_id: originator_sequence_id__.unwrap_or_default(),
                    originator_ns: originator_ns__.unwrap_or_default(),
                    payer_envelope: payer_envelope__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.UnsignedOriginatorEnvelope", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for VectorClock {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.VectorClock", len)?;
        if !self.node_id_to_sequence_id.is_empty() {
            let v: std::collections::HashMap<_, _> = self.node_id_to_sequence_id.iter()
                .map(|(k, v)| (k, v.to_string())).collect();
            struct_ser.serialize_field("nodeIdToSequenceId", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for VectorClock {
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
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = VectorClock;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.VectorClock")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<VectorClock, V::Error>
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
                    }
                }
                Ok(VectorClock {
                    node_id_to_sequence_id: node_id_to_sequence_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.VectorClock", FIELDS, GeneratedVisitor)
    }
}
