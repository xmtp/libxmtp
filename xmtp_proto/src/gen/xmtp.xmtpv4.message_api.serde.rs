// @generated
impl serde::Serialize for EnvelopesQuery {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.topics.is_empty() {
            len += 1;
        }
        if !self.originator_node_ids.is_empty() {
            len += 1;
        }
        if self.last_seen.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.message_api.EnvelopesQuery", len)?;
        if !self.topics.is_empty() {
            struct_ser.serialize_field("topics", &self.topics.iter().map(pbjson::private::base64::encode).collect::<Vec<_>>())?;
        }
        if !self.originator_node_ids.is_empty() {
            struct_ser.serialize_field("originatorNodeIds", &self.originator_node_ids)?;
        }
        if let Some(v) = self.last_seen.as_ref() {
            struct_ser.serialize_field("lastSeen", v)?;
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
            "topics",
            "originator_node_ids",
            "originatorNodeIds",
            "last_seen",
            "lastSeen",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Topics,
            OriginatorNodeIds,
            LastSeen,
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
                            "topics" => Ok(GeneratedField::Topics),
                            "originatorNodeIds" | "originator_node_ids" => Ok(GeneratedField::OriginatorNodeIds),
                            "lastSeen" | "last_seen" => Ok(GeneratedField::LastSeen),
                            _ => Ok(GeneratedField::__SkipField__),
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
                formatter.write_str("struct xmtp.xmtpv4.message_api.EnvelopesQuery")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EnvelopesQuery, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut topics__ = None;
                let mut originator_node_ids__ = None;
                let mut last_seen__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Topics => {
                            if topics__.is_some() {
                                return Err(serde::de::Error::duplicate_field("topics"));
                            }
                            topics__ = 
                                Some(map_.next_value::<Vec<::pbjson::private::BytesDeserialize<_>>>()?
                                    .into_iter().map(|x| x.0).collect())
                            ;
                        }
                        GeneratedField::OriginatorNodeIds => {
                            if originator_node_ids__.is_some() {
                                return Err(serde::de::Error::duplicate_field("originatorNodeIds"));
                            }
                            originator_node_ids__ = 
                                Some(map_.next_value::<Vec<::pbjson::private::NumberDeserialize<_>>>()?
                                    .into_iter().map(|x| x.0).collect())
                            ;
                        }
                        GeneratedField::LastSeen => {
                            if last_seen__.is_some() {
                                return Err(serde::de::Error::duplicate_field("lastSeen"));
                            }
                            last_seen__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EnvelopesQuery {
                    topics: topics__.unwrap_or_default(),
                    originator_node_ids: originator_node_ids__.unwrap_or_default(),
                    last_seen: last_seen__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.message_api.EnvelopesQuery", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.message_api.GetInboxIdsRequest", len)?;
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
                            "requests" => Ok(GeneratedField::Requests),
                            _ => Ok(GeneratedField::__SkipField__),
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
                formatter.write_str("struct xmtp.xmtpv4.message_api.GetInboxIdsRequest")
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(GetInboxIdsRequest {
                    requests: requests__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.message_api.GetInboxIdsRequest", FIELDS, GeneratedVisitor)
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
        if !self.identifier.is_empty() {
            len += 1;
        }
        if self.identifier_kind != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.message_api.GetInboxIdsRequest.Request", len)?;
        if !self.identifier.is_empty() {
            struct_ser.serialize_field("identifier", &self.identifier)?;
        }
        if self.identifier_kind != 0 {
            let v = super::super::identity::associations::IdentifierKind::try_from(self.identifier_kind)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.identifier_kind)))?;
            struct_ser.serialize_field("identifierKind", &v)?;
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
            "identifier",
            "identifier_kind",
            "identifierKind",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Identifier,
            IdentifierKind,
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
                            "identifier" => Ok(GeneratedField::Identifier),
                            "identifierKind" | "identifier_kind" => Ok(GeneratedField::IdentifierKind),
                            _ => Ok(GeneratedField::__SkipField__),
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
                formatter.write_str("struct xmtp.xmtpv4.message_api.GetInboxIdsRequest.Request")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<get_inbox_ids_request::Request, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut identifier__ = None;
                let mut identifier_kind__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Identifier => {
                            if identifier__.is_some() {
                                return Err(serde::de::Error::duplicate_field("identifier"));
                            }
                            identifier__ = Some(map_.next_value()?);
                        }
                        GeneratedField::IdentifierKind => {
                            if identifier_kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("identifierKind"));
                            }
                            identifier_kind__ = Some(map_.next_value::<super::super::identity::associations::IdentifierKind>()? as i32);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(get_inbox_ids_request::Request {
                    identifier: identifier__.unwrap_or_default(),
                    identifier_kind: identifier_kind__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.message_api.GetInboxIdsRequest.Request", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.message_api.GetInboxIdsResponse", len)?;
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
                            "responses" => Ok(GeneratedField::Responses),
                            _ => Ok(GeneratedField::__SkipField__),
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
                formatter.write_str("struct xmtp.xmtpv4.message_api.GetInboxIdsResponse")
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(GetInboxIdsResponse {
                    responses: responses__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.message_api.GetInboxIdsResponse", FIELDS, GeneratedVisitor)
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
        if !self.identifier.is_empty() {
            len += 1;
        }
        if self.inbox_id.is_some() {
            len += 1;
        }
        if self.identifier_kind != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.message_api.GetInboxIdsResponse.Response", len)?;
        if !self.identifier.is_empty() {
            struct_ser.serialize_field("identifier", &self.identifier)?;
        }
        if let Some(v) = self.inbox_id.as_ref() {
            struct_ser.serialize_field("inboxId", v)?;
        }
        if self.identifier_kind != 0 {
            let v = super::super::identity::associations::IdentifierKind::try_from(self.identifier_kind)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.identifier_kind)))?;
            struct_ser.serialize_field("identifierKind", &v)?;
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
            "identifier",
            "inbox_id",
            "inboxId",
            "identifier_kind",
            "identifierKind",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Identifier,
            InboxId,
            IdentifierKind,
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
                            "identifier" => Ok(GeneratedField::Identifier),
                            "inboxId" | "inbox_id" => Ok(GeneratedField::InboxId),
                            "identifierKind" | "identifier_kind" => Ok(GeneratedField::IdentifierKind),
                            _ => Ok(GeneratedField::__SkipField__),
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
                formatter.write_str("struct xmtp.xmtpv4.message_api.GetInboxIdsResponse.Response")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<get_inbox_ids_response::Response, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut identifier__ = None;
                let mut inbox_id__ = None;
                let mut identifier_kind__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Identifier => {
                            if identifier__.is_some() {
                                return Err(serde::de::Error::duplicate_field("identifier"));
                            }
                            identifier__ = Some(map_.next_value()?);
                        }
                        GeneratedField::InboxId => {
                            if inbox_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inboxId"));
                            }
                            inbox_id__ = map_.next_value()?;
                        }
                        GeneratedField::IdentifierKind => {
                            if identifier_kind__.is_some() {
                                return Err(serde::de::Error::duplicate_field("identifierKind"));
                            }
                            identifier_kind__ = Some(map_.next_value::<super::super::identity::associations::IdentifierKind>()? as i32);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(get_inbox_ids_response::Response {
                    identifier: identifier__.unwrap_or_default(),
                    inbox_id: inbox_id__,
                    identifier_kind: identifier_kind__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.message_api.GetInboxIdsResponse.Response", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GetNewestEnvelopeRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.topics.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.message_api.GetNewestEnvelopeRequest", len)?;
        if !self.topics.is_empty() {
            struct_ser.serialize_field("topics", &self.topics.iter().map(pbjson::private::base64::encode).collect::<Vec<_>>())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GetNewestEnvelopeRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "topics",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Topics,
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
                            "topics" => Ok(GeneratedField::Topics),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GetNewestEnvelopeRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.message_api.GetNewestEnvelopeRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GetNewestEnvelopeRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut topics__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Topics => {
                            if topics__.is_some() {
                                return Err(serde::de::Error::duplicate_field("topics"));
                            }
                            topics__ = 
                                Some(map_.next_value::<Vec<::pbjson::private::BytesDeserialize<_>>>()?
                                    .into_iter().map(|x| x.0).collect())
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(GetNewestEnvelopeRequest {
                    topics: topics__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.message_api.GetNewestEnvelopeRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GetNewestEnvelopeResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.results.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.message_api.GetNewestEnvelopeResponse", len)?;
        if !self.results.is_empty() {
            struct_ser.serialize_field("results", &self.results)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GetNewestEnvelopeResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "results",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Results,
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
                            "results" => Ok(GeneratedField::Results),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GetNewestEnvelopeResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.message_api.GetNewestEnvelopeResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GetNewestEnvelopeResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut results__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Results => {
                            if results__.is_some() {
                                return Err(serde::de::Error::duplicate_field("results"));
                            }
                            results__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(GetNewestEnvelopeResponse {
                    results: results__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.message_api.GetNewestEnvelopeResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for get_newest_envelope_response::Response {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.message_api.GetNewestEnvelopeResponse.Response", len)?;
        if let Some(v) = self.originator_envelope.as_ref() {
            struct_ser.serialize_field("originatorEnvelope", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for get_newest_envelope_response::Response {
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
                            "originatorEnvelope" | "originator_envelope" => Ok(GeneratedField::OriginatorEnvelope),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = get_newest_envelope_response::Response;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.message_api.GetNewestEnvelopeResponse.Response")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<get_newest_envelope_response::Response, V::Error>
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(get_newest_envelope_response::Response {
                    originator_envelope: originator_envelope__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.message_api.GetNewestEnvelopeResponse.Response", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for LivenessFailure {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.response_time_ns != 0 {
            len += 1;
        }
        if self.request.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.message_api.LivenessFailure", len)?;
        if self.response_time_ns != 0 {
            struct_ser.serialize_field("responseTimeNs", &self.response_time_ns)?;
        }
        if let Some(v) = self.request.as_ref() {
            match v {
                liveness_failure::Request::Subscribe(v) => {
                    struct_ser.serialize_field("subscribe", v)?;
                }
                liveness_failure::Request::Query(v) => {
                    struct_ser.serialize_field("query", v)?;
                }
                liveness_failure::Request::Publish(v) => {
                    struct_ser.serialize_field("publish", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for LivenessFailure {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "response_time_ns",
            "responseTimeNs",
            "subscribe",
            "query",
            "publish",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ResponseTimeNs,
            Subscribe,
            Query,
            Publish,
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
                            "responseTimeNs" | "response_time_ns" => Ok(GeneratedField::ResponseTimeNs),
                            "subscribe" => Ok(GeneratedField::Subscribe),
                            "query" => Ok(GeneratedField::Query),
                            "publish" => Ok(GeneratedField::Publish),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = LivenessFailure;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.message_api.LivenessFailure")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<LivenessFailure, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut response_time_ns__ = None;
                let mut request__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ResponseTimeNs => {
                            if response_time_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("responseTimeNs"));
                            }
                            response_time_ns__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Subscribe => {
                            if request__.is_some() {
                                return Err(serde::de::Error::duplicate_field("subscribe"));
                            }
                            request__ = map_.next_value::<::std::option::Option<_>>()?.map(liveness_failure::Request::Subscribe)
;
                        }
                        GeneratedField::Query => {
                            if request__.is_some() {
                                return Err(serde::de::Error::duplicate_field("query"));
                            }
                            request__ = map_.next_value::<::std::option::Option<_>>()?.map(liveness_failure::Request::Query)
;
                        }
                        GeneratedField::Publish => {
                            if request__.is_some() {
                                return Err(serde::de::Error::duplicate_field("publish"));
                            }
                            request__ = map_.next_value::<::std::option::Option<_>>()?.map(liveness_failure::Request::Publish)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(LivenessFailure {
                    response_time_ns: response_time_ns__.unwrap_or_default(),
                    request: request__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.message_api.LivenessFailure", FIELDS, GeneratedVisitor)
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
            Self::UnresponsiveNode => "MISBEHAVIOR_UNRESPONSIVE_NODE",
            Self::SlowNode => "MISBEHAVIOR_SLOW_NODE",
            Self::FailedRequest => "MISBEHAVIOR_FAILED_REQUEST",
            Self::OutOfOrder => "MISBEHAVIOR_OUT_OF_ORDER",
            Self::DuplicateSequenceId => "MISBEHAVIOR_DUPLICATE_SEQUENCE_ID",
            Self::CausalOrdering => "MISBEHAVIOR_CAUSAL_ORDERING",
            Self::InvalidPayload => "MISBEHAVIOR_INVALID_PAYLOAD",
            Self::BlockchainInconsistency => "MISBEHAVIOR_BLOCKCHAIN_INCONSISTENCY",
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
            "MISBEHAVIOR_UNRESPONSIVE_NODE",
            "MISBEHAVIOR_SLOW_NODE",
            "MISBEHAVIOR_FAILED_REQUEST",
            "MISBEHAVIOR_OUT_OF_ORDER",
            "MISBEHAVIOR_DUPLICATE_SEQUENCE_ID",
            "MISBEHAVIOR_CAUSAL_ORDERING",
            "MISBEHAVIOR_INVALID_PAYLOAD",
            "MISBEHAVIOR_BLOCKCHAIN_INCONSISTENCY",
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
                    "MISBEHAVIOR_UNRESPONSIVE_NODE" => Ok(Misbehavior::UnresponsiveNode),
                    "MISBEHAVIOR_SLOW_NODE" => Ok(Misbehavior::SlowNode),
                    "MISBEHAVIOR_FAILED_REQUEST" => Ok(Misbehavior::FailedRequest),
                    "MISBEHAVIOR_OUT_OF_ORDER" => Ok(Misbehavior::OutOfOrder),
                    "MISBEHAVIOR_DUPLICATE_SEQUENCE_ID" => Ok(Misbehavior::DuplicateSequenceId),
                    "MISBEHAVIOR_CAUSAL_ORDERING" => Ok(Misbehavior::CausalOrdering),
                    "MISBEHAVIOR_INVALID_PAYLOAD" => Ok(Misbehavior::InvalidPayload),
                    "MISBEHAVIOR_BLOCKCHAIN_INCONSISTENCY" => Ok(Misbehavior::BlockchainInconsistency),
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
        if self.server_time_ns != 0 {
            len += 1;
        }
        if !self.unsigned_misbehavior_report.is_empty() {
            len += 1;
        }
        if self.signature.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.message_api.MisbehaviorReport", len)?;
        if self.server_time_ns != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("serverTimeNs", ToString::to_string(&self.server_time_ns).as_str())?;
        }
        if !self.unsigned_misbehavior_report.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("unsignedMisbehaviorReport", pbjson::private::base64::encode(&self.unsigned_misbehavior_report).as_str())?;
        }
        if let Some(v) = self.signature.as_ref() {
            struct_ser.serialize_field("signature", v)?;
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
            "server_time_ns",
            "serverTimeNs",
            "unsigned_misbehavior_report",
            "unsignedMisbehaviorReport",
            "signature",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ServerTimeNs,
            UnsignedMisbehaviorReport,
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
                            "serverTimeNs" | "server_time_ns" => Ok(GeneratedField::ServerTimeNs),
                            "unsignedMisbehaviorReport" | "unsigned_misbehavior_report" => Ok(GeneratedField::UnsignedMisbehaviorReport),
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
            type Value = MisbehaviorReport;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.message_api.MisbehaviorReport")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<MisbehaviorReport, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut server_time_ns__ = None;
                let mut unsigned_misbehavior_report__ = None;
                let mut signature__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ServerTimeNs => {
                            if server_time_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("serverTimeNs"));
                            }
                            server_time_ns__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::UnsignedMisbehaviorReport => {
                            if unsigned_misbehavior_report__.is_some() {
                                return Err(serde::de::Error::duplicate_field("unsignedMisbehaviorReport"));
                            }
                            unsigned_misbehavior_report__ = 
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
                Ok(MisbehaviorReport {
                    server_time_ns: server_time_ns__.unwrap_or_default(),
                    unsigned_misbehavior_report: unsigned_misbehavior_report__.unwrap_or_default(),
                    signature: signature__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.message_api.MisbehaviorReport", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for PublishPayerEnvelopesRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.payer_envelopes.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.message_api.PublishPayerEnvelopesRequest", len)?;
        if !self.payer_envelopes.is_empty() {
            struct_ser.serialize_field("payerEnvelopes", &self.payer_envelopes)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for PublishPayerEnvelopesRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "payer_envelopes",
            "payerEnvelopes",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            PayerEnvelopes,
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
                            "payerEnvelopes" | "payer_envelopes" => Ok(GeneratedField::PayerEnvelopes),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = PublishPayerEnvelopesRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.message_api.PublishPayerEnvelopesRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<PublishPayerEnvelopesRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut payer_envelopes__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::PayerEnvelopes => {
                            if payer_envelopes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("payerEnvelopes"));
                            }
                            payer_envelopes__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(PublishPayerEnvelopesRequest {
                    payer_envelopes: payer_envelopes__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.message_api.PublishPayerEnvelopesRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for PublishPayerEnvelopesResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.originator_envelopes.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.message_api.PublishPayerEnvelopesResponse", len)?;
        if !self.originator_envelopes.is_empty() {
            struct_ser.serialize_field("originatorEnvelopes", &self.originator_envelopes)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for PublishPayerEnvelopesResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "originator_envelopes",
            "originatorEnvelopes",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            OriginatorEnvelopes,
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
                            "originatorEnvelopes" | "originator_envelopes" => Ok(GeneratedField::OriginatorEnvelopes),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = PublishPayerEnvelopesResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.message_api.PublishPayerEnvelopesResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<PublishPayerEnvelopesResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut originator_envelopes__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::OriginatorEnvelopes => {
                            if originator_envelopes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("originatorEnvelopes"));
                            }
                            originator_envelopes__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(PublishPayerEnvelopesResponse {
                    originator_envelopes: originator_envelopes__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.message_api.PublishPayerEnvelopesResponse", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.message_api.QueryEnvelopesRequest", len)?;
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
                            "query" => Ok(GeneratedField::Query),
                            "limit" => Ok(GeneratedField::Limit),
                            _ => Ok(GeneratedField::__SkipField__),
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
                formatter.write_str("struct xmtp.xmtpv4.message_api.QueryEnvelopesRequest")
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(QueryEnvelopesRequest {
                    query: query__,
                    limit: limit__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.message_api.QueryEnvelopesRequest", FIELDS, GeneratedVisitor)
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
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.message_api.QueryEnvelopesResponse", len)?;
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
                            "envelopes" => Ok(GeneratedField::Envelopes),
                            _ => Ok(GeneratedField::__SkipField__),
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
                formatter.write_str("struct xmtp.xmtpv4.message_api.QueryEnvelopesResponse")
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(QueryEnvelopesResponse {
                    envelopes: envelopes__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.message_api.QueryEnvelopesResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for QueryMisbehaviorReportsRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.after_ns != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.message_api.QueryMisbehaviorReportsRequest", len)?;
        if self.after_ns != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("afterNs", ToString::to_string(&self.after_ns).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for QueryMisbehaviorReportsRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "after_ns",
            "afterNs",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AfterNs,
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
                            "afterNs" | "after_ns" => Ok(GeneratedField::AfterNs),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = QueryMisbehaviorReportsRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.message_api.QueryMisbehaviorReportsRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<QueryMisbehaviorReportsRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut after_ns__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AfterNs => {
                            if after_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("afterNs"));
                            }
                            after_ns__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(QueryMisbehaviorReportsRequest {
                    after_ns: after_ns__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.message_api.QueryMisbehaviorReportsRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for QueryMisbehaviorReportsResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.reports.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.message_api.QueryMisbehaviorReportsResponse", len)?;
        if !self.reports.is_empty() {
            struct_ser.serialize_field("reports", &self.reports)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for QueryMisbehaviorReportsResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "reports",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Reports,
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
                            "reports" => Ok(GeneratedField::Reports),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = QueryMisbehaviorReportsResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.message_api.QueryMisbehaviorReportsResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<QueryMisbehaviorReportsResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut reports__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Reports => {
                            if reports__.is_some() {
                                return Err(serde::de::Error::duplicate_field("reports"));
                            }
                            reports__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(QueryMisbehaviorReportsResponse {
                    reports: reports__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.message_api.QueryMisbehaviorReportsResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SafetyFailure {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.message_api.SafetyFailure", len)?;
        if !self.envelopes.is_empty() {
            struct_ser.serialize_field("envelopes", &self.envelopes)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SafetyFailure {
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
                            "envelopes" => Ok(GeneratedField::Envelopes),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SafetyFailure;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.message_api.SafetyFailure")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SafetyFailure, V::Error>
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(SafetyFailure {
                    envelopes: envelopes__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.message_api.SafetyFailure", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SubmitMisbehaviorReportRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.report.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.message_api.SubmitMisbehaviorReportRequest", len)?;
        if let Some(v) = self.report.as_ref() {
            struct_ser.serialize_field("report", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SubmitMisbehaviorReportRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "report",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Report,
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
                            "report" => Ok(GeneratedField::Report),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SubmitMisbehaviorReportRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.message_api.SubmitMisbehaviorReportRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SubmitMisbehaviorReportRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut report__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Report => {
                            if report__.is_some() {
                                return Err(serde::de::Error::duplicate_field("report"));
                            }
                            report__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(SubmitMisbehaviorReportRequest {
                    report: report__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.message_api.SubmitMisbehaviorReportRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SubmitMisbehaviorReportResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("xmtp.xmtpv4.message_api.SubmitMisbehaviorReportResponse", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SubmitMisbehaviorReportResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
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
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SubmitMisbehaviorReportResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.message_api.SubmitMisbehaviorReportResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SubmitMisbehaviorReportResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(SubmitMisbehaviorReportResponse {
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.message_api.SubmitMisbehaviorReportResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SubscribeEnvelopesRequest {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.message_api.SubscribeEnvelopesRequest", len)?;
        if let Some(v) = self.query.as_ref() {
            struct_ser.serialize_field("query", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SubscribeEnvelopesRequest {
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
                            "query" => Ok(GeneratedField::Query),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SubscribeEnvelopesRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.message_api.SubscribeEnvelopesRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SubscribeEnvelopesRequest, V::Error>
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(SubscribeEnvelopesRequest {
                    query: query__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.message_api.SubscribeEnvelopesRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SubscribeEnvelopesResponse {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.message_api.SubscribeEnvelopesResponse", len)?;
        if !self.envelopes.is_empty() {
            struct_ser.serialize_field("envelopes", &self.envelopes)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SubscribeEnvelopesResponse {
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
                            "envelopes" => Ok(GeneratedField::Envelopes),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SubscribeEnvelopesResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.message_api.SubscribeEnvelopesResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SubscribeEnvelopesResponse, V::Error>
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
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(SubscribeEnvelopesResponse {
                    envelopes: envelopes__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.message_api.SubscribeEnvelopesResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for UnsignedMisbehaviorReport {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.reporter_time_ns != 0 {
            len += 1;
        }
        if self.misbehaving_node_id != 0 {
            len += 1;
        }
        if self.r#type != 0 {
            len += 1;
        }
        if self.submitted_by_node {
            len += 1;
        }
        if self.failure.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.xmtpv4.message_api.UnsignedMisbehaviorReport", len)?;
        if self.reporter_time_ns != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("reporterTimeNs", ToString::to_string(&self.reporter_time_ns).as_str())?;
        }
        if self.misbehaving_node_id != 0 {
            struct_ser.serialize_field("misbehavingNodeId", &self.misbehaving_node_id)?;
        }
        if self.r#type != 0 {
            let v = Misbehavior::try_from(self.r#type)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.r#type)))?;
            struct_ser.serialize_field("type", &v)?;
        }
        if self.submitted_by_node {
            struct_ser.serialize_field("submittedByNode", &self.submitted_by_node)?;
        }
        if let Some(v) = self.failure.as_ref() {
            match v {
                unsigned_misbehavior_report::Failure::Liveness(v) => {
                    struct_ser.serialize_field("liveness", v)?;
                }
                unsigned_misbehavior_report::Failure::Safety(v) => {
                    struct_ser.serialize_field("safety", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for UnsignedMisbehaviorReport {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "reporter_time_ns",
            "reporterTimeNs",
            "misbehaving_node_id",
            "misbehavingNodeId",
            "type",
            "submitted_by_node",
            "submittedByNode",
            "liveness",
            "safety",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ReporterTimeNs,
            MisbehavingNodeId,
            Type,
            SubmittedByNode,
            Liveness,
            Safety,
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
                            "reporterTimeNs" | "reporter_time_ns" => Ok(GeneratedField::ReporterTimeNs),
                            "misbehavingNodeId" | "misbehaving_node_id" => Ok(GeneratedField::MisbehavingNodeId),
                            "type" => Ok(GeneratedField::Type),
                            "submittedByNode" | "submitted_by_node" => Ok(GeneratedField::SubmittedByNode),
                            "liveness" => Ok(GeneratedField::Liveness),
                            "safety" => Ok(GeneratedField::Safety),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = UnsignedMisbehaviorReport;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.xmtpv4.message_api.UnsignedMisbehaviorReport")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<UnsignedMisbehaviorReport, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut reporter_time_ns__ = None;
                let mut misbehaving_node_id__ = None;
                let mut r#type__ = None;
                let mut submitted_by_node__ = None;
                let mut failure__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ReporterTimeNs => {
                            if reporter_time_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("reporterTimeNs"));
                            }
                            reporter_time_ns__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::MisbehavingNodeId => {
                            if misbehaving_node_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("misbehavingNodeId"));
                            }
                            misbehaving_node_id__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Type => {
                            if r#type__.is_some() {
                                return Err(serde::de::Error::duplicate_field("type"));
                            }
                            r#type__ = Some(map_.next_value::<Misbehavior>()? as i32);
                        }
                        GeneratedField::SubmittedByNode => {
                            if submitted_by_node__.is_some() {
                                return Err(serde::de::Error::duplicate_field("submittedByNode"));
                            }
                            submitted_by_node__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Liveness => {
                            if failure__.is_some() {
                                return Err(serde::de::Error::duplicate_field("liveness"));
                            }
                            failure__ = map_.next_value::<::std::option::Option<_>>()?.map(unsigned_misbehavior_report::Failure::Liveness)
;
                        }
                        GeneratedField::Safety => {
                            if failure__.is_some() {
                                return Err(serde::de::Error::duplicate_field("safety"));
                            }
                            failure__ = map_.next_value::<::std::option::Option<_>>()?.map(unsigned_misbehavior_report::Failure::Safety)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(UnsignedMisbehaviorReport {
                    reporter_time_ns: reporter_time_ns__.unwrap_or_default(),
                    misbehaving_node_id: misbehaving_node_id__.unwrap_or_default(),
                    r#type: r#type__.unwrap_or_default(),
                    submitted_by_node: submitted_by_node__.unwrap_or_default(),
                    failure: failure__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.xmtpv4.message_api.UnsignedMisbehaviorReport", FIELDS, GeneratedVisitor)
    }
}
