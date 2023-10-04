// @generated
impl serde::Serialize for CreateAuthTokenRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.timestamp_ns.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.CreateAuthTokenRequest", len)?;
        if let Some(v) = self.timestamp_ns.as_ref() {
            struct_ser.serialize_field("timestampNs", ToString::to_string(&v).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for CreateAuthTokenRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "timestamp_ns",
            "timestampNs",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            TimestampNs,
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
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = CreateAuthTokenRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.CreateAuthTokenRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<CreateAuthTokenRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut timestamp_ns__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::TimestampNs => {
                            if timestamp_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("timestampNs"));
                            }
                            timestamp_ns__ = 
                                map.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| x.0)
                            ;
                        }
                    }
                }
                Ok(CreateAuthTokenRequest {
                    timestamp_ns: timestamp_ns__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.CreateAuthTokenRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for CreateInviteFromTopicRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.recipient.is_some() {
            len += 1;
        }
        if !self.content_topic.is_empty() {
            len += 1;
        }
        if self.created_ns != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.CreateInviteFromTopicRequest", len)?;
        if let Some(v) = self.recipient.as_ref() {
            struct_ser.serialize_field("recipient", v)?;
        }
        if !self.content_topic.is_empty() {
            struct_ser.serialize_field("contentTopic", &self.content_topic)?;
        }
        if self.created_ns != 0 {
            struct_ser.serialize_field("createdNs", ToString::to_string(&self.created_ns).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for CreateInviteFromTopicRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "recipient",
            "content_topic",
            "contentTopic",
            "created_ns",
            "createdNs",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Recipient,
            ContentTopic,
            CreatedNs,
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
                            "recipient" => Ok(GeneratedField::Recipient),
                            "contentTopic" | "content_topic" => Ok(GeneratedField::ContentTopic),
                            "createdNs" | "created_ns" => Ok(GeneratedField::CreatedNs),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = CreateInviteFromTopicRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.CreateInviteFromTopicRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<CreateInviteFromTopicRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut recipient__ = None;
                let mut content_topic__ = None;
                let mut created_ns__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Recipient => {
                            if recipient__.is_some() {
                                return Err(serde::de::Error::duplicate_field("recipient"));
                            }
                            recipient__ = map.next_value()?;
                        }
                        GeneratedField::ContentTopic => {
                            if content_topic__.is_some() {
                                return Err(serde::de::Error::duplicate_field("contentTopic"));
                            }
                            content_topic__ = Some(map.next_value()?);
                        }
                        GeneratedField::CreatedNs => {
                            if created_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createdNs"));
                            }
                            created_ns__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(CreateInviteFromTopicRequest {
                    recipient: recipient__,
                    content_topic: content_topic__.unwrap_or_default(),
                    created_ns: created_ns__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.CreateInviteFromTopicRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for CreateInviteRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.context.is_some() {
            len += 1;
        }
        if self.recipient.is_some() {
            len += 1;
        }
        if self.created_ns != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.CreateInviteRequest", len)?;
        if let Some(v) = self.context.as_ref() {
            struct_ser.serialize_field("context", v)?;
        }
        if let Some(v) = self.recipient.as_ref() {
            struct_ser.serialize_field("recipient", v)?;
        }
        if self.created_ns != 0 {
            struct_ser.serialize_field("createdNs", ToString::to_string(&self.created_ns).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for CreateInviteRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "context",
            "recipient",
            "created_ns",
            "createdNs",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Context,
            Recipient,
            CreatedNs,
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
                            "context" => Ok(GeneratedField::Context),
                            "recipient" => Ok(GeneratedField::Recipient),
                            "createdNs" | "created_ns" => Ok(GeneratedField::CreatedNs),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = CreateInviteRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.CreateInviteRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<CreateInviteRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut context__ = None;
                let mut recipient__ = None;
                let mut created_ns__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Context => {
                            if context__.is_some() {
                                return Err(serde::de::Error::duplicate_field("context"));
                            }
                            context__ = map.next_value()?;
                        }
                        GeneratedField::Recipient => {
                            if recipient__.is_some() {
                                return Err(serde::de::Error::duplicate_field("recipient"));
                            }
                            recipient__ = map.next_value()?;
                        }
                        GeneratedField::CreatedNs => {
                            if created_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createdNs"));
                            }
                            created_ns__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(CreateInviteRequest {
                    context: context__,
                    recipient: recipient__,
                    created_ns: created_ns__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.CreateInviteRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for CreateInviteResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.conversation.is_some() {
            len += 1;
        }
        if !self.payload.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.CreateInviteResponse", len)?;
        if let Some(v) = self.conversation.as_ref() {
            struct_ser.serialize_field("conversation", v)?;
        }
        if !self.payload.is_empty() {
            struct_ser.serialize_field("payload", pbjson::private::base64::encode(&self.payload).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for CreateInviteResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "conversation",
            "payload",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Conversation,
            Payload,
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
                            "conversation" => Ok(GeneratedField::Conversation),
                            "payload" => Ok(GeneratedField::Payload),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = CreateInviteResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.CreateInviteResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<CreateInviteResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut conversation__ = None;
                let mut payload__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Conversation => {
                            if conversation__.is_some() {
                                return Err(serde::de::Error::duplicate_field("conversation"));
                            }
                            conversation__ = map.next_value()?;
                        }
                        GeneratedField::Payload => {
                            if payload__.is_some() {
                                return Err(serde::de::Error::duplicate_field("payload"));
                            }
                            payload__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(CreateInviteResponse {
                    conversation: conversation__,
                    payload: payload__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.CreateInviteResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for CreateInvitesRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.context.is_some() {
            len += 1;
        }
        if !self.recipients.is_empty() {
            len += 1;
        }
        if self.created_ns != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.CreateInvitesRequest", len)?;
        if let Some(v) = self.context.as_ref() {
            struct_ser.serialize_field("context", v)?;
        }
        if !self.recipients.is_empty() {
            struct_ser.serialize_field("recipients", &self.recipients)?;
        }
        if self.created_ns != 0 {
            struct_ser.serialize_field("createdNs", ToString::to_string(&self.created_ns).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for CreateInvitesRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "context",
            "recipients",
            "created_ns",
            "createdNs",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Context,
            Recipients,
            CreatedNs,
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
                            "context" => Ok(GeneratedField::Context),
                            "recipients" => Ok(GeneratedField::Recipients),
                            "createdNs" | "created_ns" => Ok(GeneratedField::CreatedNs),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = CreateInvitesRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.CreateInvitesRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<CreateInvitesRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut context__ = None;
                let mut recipients__ = None;
                let mut created_ns__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Context => {
                            if context__.is_some() {
                                return Err(serde::de::Error::duplicate_field("context"));
                            }
                            context__ = map.next_value()?;
                        }
                        GeneratedField::Recipients => {
                            if recipients__.is_some() {
                                return Err(serde::de::Error::duplicate_field("recipients"));
                            }
                            recipients__ = Some(map.next_value()?);
                        }
                        GeneratedField::CreatedNs => {
                            if created_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createdNs"));
                            }
                            created_ns__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(CreateInvitesRequest {
                    context: context__,
                    recipients: recipients__.unwrap_or_default(),
                    created_ns: created_ns__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.CreateInvitesRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for CreateInvitesResponse {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.CreateInvitesResponse", len)?;
        if !self.responses.is_empty() {
            struct_ser.serialize_field("responses", &self.responses)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for CreateInvitesResponse {
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
            type Value = CreateInvitesResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.CreateInvitesResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<CreateInvitesResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut responses__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Responses => {
                            if responses__.is_some() {
                                return Err(serde::de::Error::duplicate_field("responses"));
                            }
                            responses__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(CreateInvitesResponse {
                    responses: responses__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.CreateInvitesResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for DecryptResponse {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.DecryptResponse", len)?;
        if !self.responses.is_empty() {
            struct_ser.serialize_field("responses", &self.responses)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for DecryptResponse {
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
            type Value = DecryptResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.DecryptResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<DecryptResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut responses__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Responses => {
                            if responses__.is_some() {
                                return Err(serde::de::Error::duplicate_field("responses"));
                            }
                            responses__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(DecryptResponse {
                    responses: responses__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.DecryptResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for decrypt_response::Response {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.response.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.DecryptResponse.Response", len)?;
        if let Some(v) = self.response.as_ref() {
            match v {
                decrypt_response::response::Response::Result(v) => {
                    struct_ser.serialize_field("result", v)?;
                }
                decrypt_response::response::Response::Error(v) => {
                    struct_ser.serialize_field("error", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for decrypt_response::Response {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "result",
            "error",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Result,
            Error,
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
                            "result" => Ok(GeneratedField::Result),
                            "error" => Ok(GeneratedField::Error),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = decrypt_response::Response;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.DecryptResponse.Response")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<decrypt_response::Response, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut response__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Result => {
                            if response__.is_some() {
                                return Err(serde::de::Error::duplicate_field("result"));
                            }
                            response__ = map.next_value::<::std::option::Option<_>>()?.map(decrypt_response::response::Response::Result)
;
                        }
                        GeneratedField::Error => {
                            if response__.is_some() {
                                return Err(serde::de::Error::duplicate_field("error"));
                            }
                            response__ = map.next_value::<::std::option::Option<_>>()?.map(decrypt_response::response::Response::Error)
;
                        }
                    }
                }
                Ok(decrypt_response::Response {
                    response: response__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.DecryptResponse.Response", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for decrypt_response::response::Success {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.decrypted.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.DecryptResponse.Response.Success", len)?;
        if !self.decrypted.is_empty() {
            struct_ser.serialize_field("decrypted", pbjson::private::base64::encode(&self.decrypted).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for decrypt_response::response::Success {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "decrypted",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Decrypted,
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
                            "decrypted" => Ok(GeneratedField::Decrypted),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = decrypt_response::response::Success;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.DecryptResponse.Response.Success")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<decrypt_response::response::Success, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut decrypted__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Decrypted => {
                            if decrypted__.is_some() {
                                return Err(serde::de::Error::duplicate_field("decrypted"));
                            }
                            decrypted__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(decrypt_response::response::Success {
                    decrypted: decrypted__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.DecryptResponse.Response.Success", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for DecryptV1Request {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.DecryptV1Request", len)?;
        if !self.requests.is_empty() {
            struct_ser.serialize_field("requests", &self.requests)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for DecryptV1Request {
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
            type Value = DecryptV1Request;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.DecryptV1Request")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<DecryptV1Request, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut requests__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Requests => {
                            if requests__.is_some() {
                                return Err(serde::de::Error::duplicate_field("requests"));
                            }
                            requests__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(DecryptV1Request {
                    requests: requests__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.DecryptV1Request", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for decrypt_v1_request::Request {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.payload.is_some() {
            len += 1;
        }
        if self.peer_keys.is_some() {
            len += 1;
        }
        if !self.header_bytes.is_empty() {
            len += 1;
        }
        if self.is_sender {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.DecryptV1Request.Request", len)?;
        if let Some(v) = self.payload.as_ref() {
            struct_ser.serialize_field("payload", v)?;
        }
        if let Some(v) = self.peer_keys.as_ref() {
            struct_ser.serialize_field("peerKeys", v)?;
        }
        if !self.header_bytes.is_empty() {
            struct_ser.serialize_field("headerBytes", pbjson::private::base64::encode(&self.header_bytes).as_str())?;
        }
        if self.is_sender {
            struct_ser.serialize_field("isSender", &self.is_sender)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for decrypt_v1_request::Request {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "payload",
            "peer_keys",
            "peerKeys",
            "header_bytes",
            "headerBytes",
            "is_sender",
            "isSender",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Payload,
            PeerKeys,
            HeaderBytes,
            IsSender,
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
                            "payload" => Ok(GeneratedField::Payload),
                            "peerKeys" | "peer_keys" => Ok(GeneratedField::PeerKeys),
                            "headerBytes" | "header_bytes" => Ok(GeneratedField::HeaderBytes),
                            "isSender" | "is_sender" => Ok(GeneratedField::IsSender),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = decrypt_v1_request::Request;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.DecryptV1Request.Request")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<decrypt_v1_request::Request, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut payload__ = None;
                let mut peer_keys__ = None;
                let mut header_bytes__ = None;
                let mut is_sender__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Payload => {
                            if payload__.is_some() {
                                return Err(serde::de::Error::duplicate_field("payload"));
                            }
                            payload__ = map.next_value()?;
                        }
                        GeneratedField::PeerKeys => {
                            if peer_keys__.is_some() {
                                return Err(serde::de::Error::duplicate_field("peerKeys"));
                            }
                            peer_keys__ = map.next_value()?;
                        }
                        GeneratedField::HeaderBytes => {
                            if header_bytes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("headerBytes"));
                            }
                            header_bytes__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::IsSender => {
                            if is_sender__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isSender"));
                            }
                            is_sender__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(decrypt_v1_request::Request {
                    payload: payload__,
                    peer_keys: peer_keys__,
                    header_bytes: header_bytes__.unwrap_or_default(),
                    is_sender: is_sender__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.DecryptV1Request.Request", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for DecryptV2Request {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.DecryptV2Request", len)?;
        if !self.requests.is_empty() {
            struct_ser.serialize_field("requests", &self.requests)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for DecryptV2Request {
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
            type Value = DecryptV2Request;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.DecryptV2Request")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<DecryptV2Request, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut requests__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Requests => {
                            if requests__.is_some() {
                                return Err(serde::de::Error::duplicate_field("requests"));
                            }
                            requests__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(DecryptV2Request {
                    requests: requests__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.DecryptV2Request", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for decrypt_v2_request::Request {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.payload.is_some() {
            len += 1;
        }
        if !self.header_bytes.is_empty() {
            len += 1;
        }
        if !self.content_topic.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.DecryptV2Request.Request", len)?;
        if let Some(v) = self.payload.as_ref() {
            struct_ser.serialize_field("payload", v)?;
        }
        if !self.header_bytes.is_empty() {
            struct_ser.serialize_field("headerBytes", pbjson::private::base64::encode(&self.header_bytes).as_str())?;
        }
        if !self.content_topic.is_empty() {
            struct_ser.serialize_field("contentTopic", &self.content_topic)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for decrypt_v2_request::Request {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "payload",
            "header_bytes",
            "headerBytes",
            "content_topic",
            "contentTopic",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Payload,
            HeaderBytes,
            ContentTopic,
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
                            "payload" => Ok(GeneratedField::Payload),
                            "headerBytes" | "header_bytes" => Ok(GeneratedField::HeaderBytes),
                            "contentTopic" | "content_topic" => Ok(GeneratedField::ContentTopic),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = decrypt_v2_request::Request;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.DecryptV2Request.Request")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<decrypt_v2_request::Request, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut payload__ = None;
                let mut header_bytes__ = None;
                let mut content_topic__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Payload => {
                            if payload__.is_some() {
                                return Err(serde::de::Error::duplicate_field("payload"));
                            }
                            payload__ = map.next_value()?;
                        }
                        GeneratedField::HeaderBytes => {
                            if header_bytes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("headerBytes"));
                            }
                            header_bytes__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ContentTopic => {
                            if content_topic__.is_some() {
                                return Err(serde::de::Error::duplicate_field("contentTopic"));
                            }
                            content_topic__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(decrypt_v2_request::Request {
                    payload: payload__,
                    header_bytes: header_bytes__.unwrap_or_default(),
                    content_topic: content_topic__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.DecryptV2Request.Request", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EncryptResponse {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.EncryptResponse", len)?;
        if !self.responses.is_empty() {
            struct_ser.serialize_field("responses", &self.responses)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EncryptResponse {
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
            type Value = EncryptResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.EncryptResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<EncryptResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut responses__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Responses => {
                            if responses__.is_some() {
                                return Err(serde::de::Error::duplicate_field("responses"));
                            }
                            responses__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(EncryptResponse {
                    responses: responses__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.EncryptResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for encrypt_response::Response {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.response.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.EncryptResponse.Response", len)?;
        if let Some(v) = self.response.as_ref() {
            match v {
                encrypt_response::response::Response::Result(v) => {
                    struct_ser.serialize_field("result", v)?;
                }
                encrypt_response::response::Response::Error(v) => {
                    struct_ser.serialize_field("error", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for encrypt_response::Response {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "result",
            "error",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Result,
            Error,
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
                            "result" => Ok(GeneratedField::Result),
                            "error" => Ok(GeneratedField::Error),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = encrypt_response::Response;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.EncryptResponse.Response")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<encrypt_response::Response, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut response__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Result => {
                            if response__.is_some() {
                                return Err(serde::de::Error::duplicate_field("result"));
                            }
                            response__ = map.next_value::<::std::option::Option<_>>()?.map(encrypt_response::response::Response::Result)
;
                        }
                        GeneratedField::Error => {
                            if response__.is_some() {
                                return Err(serde::de::Error::duplicate_field("error"));
                            }
                            response__ = map.next_value::<::std::option::Option<_>>()?.map(encrypt_response::response::Response::Error)
;
                        }
                    }
                }
                Ok(encrypt_response::Response {
                    response: response__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.EncryptResponse.Response", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for encrypt_response::response::Success {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.encrypted.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.EncryptResponse.Response.Success", len)?;
        if let Some(v) = self.encrypted.as_ref() {
            struct_ser.serialize_field("encrypted", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for encrypt_response::response::Success {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "encrypted",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Encrypted,
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
                            "encrypted" => Ok(GeneratedField::Encrypted),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = encrypt_response::response::Success;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.EncryptResponse.Response.Success")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<encrypt_response::response::Success, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut encrypted__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Encrypted => {
                            if encrypted__.is_some() {
                                return Err(serde::de::Error::duplicate_field("encrypted"));
                            }
                            encrypted__ = map.next_value()?;
                        }
                    }
                }
                Ok(encrypt_response::response::Success {
                    encrypted: encrypted__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.EncryptResponse.Response.Success", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EncryptV1Request {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.EncryptV1Request", len)?;
        if !self.requests.is_empty() {
            struct_ser.serialize_field("requests", &self.requests)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EncryptV1Request {
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
            type Value = EncryptV1Request;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.EncryptV1Request")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<EncryptV1Request, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut requests__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Requests => {
                            if requests__.is_some() {
                                return Err(serde::de::Error::duplicate_field("requests"));
                            }
                            requests__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(EncryptV1Request {
                    requests: requests__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.EncryptV1Request", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for encrypt_v1_request::Request {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.recipient.is_some() {
            len += 1;
        }
        if !self.payload.is_empty() {
            len += 1;
        }
        if !self.header_bytes.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.EncryptV1Request.Request", len)?;
        if let Some(v) = self.recipient.as_ref() {
            struct_ser.serialize_field("recipient", v)?;
        }
        if !self.payload.is_empty() {
            struct_ser.serialize_field("payload", pbjson::private::base64::encode(&self.payload).as_str())?;
        }
        if !self.header_bytes.is_empty() {
            struct_ser.serialize_field("headerBytes", pbjson::private::base64::encode(&self.header_bytes).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for encrypt_v1_request::Request {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "recipient",
            "payload",
            "header_bytes",
            "headerBytes",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Recipient,
            Payload,
            HeaderBytes,
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
                            "recipient" => Ok(GeneratedField::Recipient),
                            "payload" => Ok(GeneratedField::Payload),
                            "headerBytes" | "header_bytes" => Ok(GeneratedField::HeaderBytes),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = encrypt_v1_request::Request;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.EncryptV1Request.Request")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<encrypt_v1_request::Request, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut recipient__ = None;
                let mut payload__ = None;
                let mut header_bytes__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Recipient => {
                            if recipient__.is_some() {
                                return Err(serde::de::Error::duplicate_field("recipient"));
                            }
                            recipient__ = map.next_value()?;
                        }
                        GeneratedField::Payload => {
                            if payload__.is_some() {
                                return Err(serde::de::Error::duplicate_field("payload"));
                            }
                            payload__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::HeaderBytes => {
                            if header_bytes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("headerBytes"));
                            }
                            header_bytes__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(encrypt_v1_request::Request {
                    recipient: recipient__,
                    payload: payload__.unwrap_or_default(),
                    header_bytes: header_bytes__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.EncryptV1Request.Request", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EncryptV2Request {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.EncryptV2Request", len)?;
        if !self.requests.is_empty() {
            struct_ser.serialize_field("requests", &self.requests)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EncryptV2Request {
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
            type Value = EncryptV2Request;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.EncryptV2Request")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<EncryptV2Request, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut requests__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Requests => {
                            if requests__.is_some() {
                                return Err(serde::de::Error::duplicate_field("requests"));
                            }
                            requests__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(EncryptV2Request {
                    requests: requests__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.EncryptV2Request", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for encrypt_v2_request::Request {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.payload.is_empty() {
            len += 1;
        }
        if !self.header_bytes.is_empty() {
            len += 1;
        }
        if !self.content_topic.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.EncryptV2Request.Request", len)?;
        if !self.payload.is_empty() {
            struct_ser.serialize_field("payload", pbjson::private::base64::encode(&self.payload).as_str())?;
        }
        if !self.header_bytes.is_empty() {
            struct_ser.serialize_field("headerBytes", pbjson::private::base64::encode(&self.header_bytes).as_str())?;
        }
        if !self.content_topic.is_empty() {
            struct_ser.serialize_field("contentTopic", &self.content_topic)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for encrypt_v2_request::Request {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "payload",
            "header_bytes",
            "headerBytes",
            "content_topic",
            "contentTopic",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Payload,
            HeaderBytes,
            ContentTopic,
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
                            "payload" => Ok(GeneratedField::Payload),
                            "headerBytes" | "header_bytes" => Ok(GeneratedField::HeaderBytes),
                            "contentTopic" | "content_topic" => Ok(GeneratedField::ContentTopic),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = encrypt_v2_request::Request;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.EncryptV2Request.Request")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<encrypt_v2_request::Request, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut payload__ = None;
                let mut header_bytes__ = None;
                let mut content_topic__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Payload => {
                            if payload__.is_some() {
                                return Err(serde::de::Error::duplicate_field("payload"));
                            }
                            payload__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::HeaderBytes => {
                            if header_bytes__.is_some() {
                                return Err(serde::de::Error::duplicate_field("headerBytes"));
                            }
                            header_bytes__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::ContentTopic => {
                            if content_topic__.is_some() {
                                return Err(serde::de::Error::duplicate_field("contentTopic"));
                            }
                            content_topic__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(encrypt_v2_request::Request {
                    payload: payload__.unwrap_or_default(),
                    header_bytes: header_bytes__.unwrap_or_default(),
                    content_topic: content_topic__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.EncryptV2Request.Request", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ErrorCode {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "ERROR_CODE_UNSPECIFIED",
            Self::InvalidInput => "ERROR_CODE_INVALID_INPUT",
            Self::NoMatchingPrekey => "ERROR_CODE_NO_MATCHING_PREKEY",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ErrorCode {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "ERROR_CODE_UNSPECIFIED",
            "ERROR_CODE_INVALID_INPUT",
            "ERROR_CODE_NO_MATCHING_PREKEY",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ErrorCode;

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
                    .and_then(ErrorCode::from_i32)
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
                    .and_then(ErrorCode::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "ERROR_CODE_UNSPECIFIED" => Ok(ErrorCode::Unspecified),
                    "ERROR_CODE_INVALID_INPUT" => Ok(ErrorCode::InvalidInput),
                    "ERROR_CODE_NO_MATCHING_PREKEY" => Ok(ErrorCode::NoMatchingPrekey),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for GetKeystoreStatusRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.wallet_address.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.GetKeystoreStatusRequest", len)?;
        if !self.wallet_address.is_empty() {
            struct_ser.serialize_field("walletAddress", &self.wallet_address)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GetKeystoreStatusRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "wallet_address",
            "walletAddress",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            WalletAddress,
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
                            "walletAddress" | "wallet_address" => Ok(GeneratedField::WalletAddress),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GetKeystoreStatusRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.GetKeystoreStatusRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<GetKeystoreStatusRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut wallet_address__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::WalletAddress => {
                            if wallet_address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("walletAddress"));
                            }
                            wallet_address__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(GetKeystoreStatusRequest {
                    wallet_address: wallet_address__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.GetKeystoreStatusRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GetKeystoreStatusResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.status != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.GetKeystoreStatusResponse", len)?;
        if self.status != 0 {
            let v = get_keystore_status_response::KeystoreStatus::from_i32(self.status)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.status)))?;
            struct_ser.serialize_field("status", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GetKeystoreStatusResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "status",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Status,
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
                            "status" => Ok(GeneratedField::Status),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GetKeystoreStatusResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.GetKeystoreStatusResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<GetKeystoreStatusResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut status__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Status => {
                            if status__.is_some() {
                                return Err(serde::de::Error::duplicate_field("status"));
                            }
                            status__ = Some(map.next_value::<get_keystore_status_response::KeystoreStatus>()? as i32);
                        }
                    }
                }
                Ok(GetKeystoreStatusResponse {
                    status: status__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.GetKeystoreStatusResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for get_keystore_status_response::KeystoreStatus {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unspecified => "KEYSTORE_STATUS_UNSPECIFIED",
            Self::Uninitialized => "KEYSTORE_STATUS_UNINITIALIZED",
            Self::Initialized => "KEYSTORE_STATUS_INITIALIZED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for get_keystore_status_response::KeystoreStatus {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "KEYSTORE_STATUS_UNSPECIFIED",
            "KEYSTORE_STATUS_UNINITIALIZED",
            "KEYSTORE_STATUS_INITIALIZED",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = get_keystore_status_response::KeystoreStatus;

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
                    .and_then(get_keystore_status_response::KeystoreStatus::from_i32)
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
                    .and_then(get_keystore_status_response::KeystoreStatus::from_i32)
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "KEYSTORE_STATUS_UNSPECIFIED" => Ok(get_keystore_status_response::KeystoreStatus::Unspecified),
                    "KEYSTORE_STATUS_UNINITIALIZED" => Ok(get_keystore_status_response::KeystoreStatus::Uninitialized),
                    "KEYSTORE_STATUS_INITIALIZED" => Ok(get_keystore_status_response::KeystoreStatus::Initialized),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for GetV2ConversationsResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.conversations.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.GetV2ConversationsResponse", len)?;
        if !self.conversations.is_empty() {
            struct_ser.serialize_field("conversations", &self.conversations)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GetV2ConversationsResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "conversations",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Conversations,
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
                            "conversations" => Ok(GeneratedField::Conversations),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GetV2ConversationsResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.GetV2ConversationsResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<GetV2ConversationsResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut conversations__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Conversations => {
                            if conversations__.is_some() {
                                return Err(serde::de::Error::duplicate_field("conversations"));
                            }
                            conversations__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(GetV2ConversationsResponse {
                    conversations: conversations__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.GetV2ConversationsResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for InitKeystoreRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.bundle.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.InitKeystoreRequest", len)?;
        if let Some(v) = self.bundle.as_ref() {
            match v {
                init_keystore_request::Bundle::V1(v) => {
                    struct_ser.serialize_field("v1", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for InitKeystoreRequest {
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
            type Value = InitKeystoreRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.InitKeystoreRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<InitKeystoreRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut bundle__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::V1 => {
                            if bundle__.is_some() {
                                return Err(serde::de::Error::duplicate_field("v1"));
                            }
                            bundle__ = map.next_value::<::std::option::Option<_>>()?.map(init_keystore_request::Bundle::V1)
;
                        }
                    }
                }
                Ok(InitKeystoreRequest {
                    bundle: bundle__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.InitKeystoreRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for InitKeystoreResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.error.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.InitKeystoreResponse", len)?;
        if let Some(v) = self.error.as_ref() {
            struct_ser.serialize_field("error", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for InitKeystoreResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "error",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Error,
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
                            "error" => Ok(GeneratedField::Error),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = InitKeystoreResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.InitKeystoreResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<InitKeystoreResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut error__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Error => {
                            if error__.is_some() {
                                return Err(serde::de::Error::duplicate_field("error"));
                            }
                            error__ = map.next_value()?;
                        }
                    }
                }
                Ok(InitKeystoreResponse {
                    error: error__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.InitKeystoreResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for KeystoreError {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.message.is_empty() {
            len += 1;
        }
        if self.code != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.KeystoreError", len)?;
        if !self.message.is_empty() {
            struct_ser.serialize_field("message", &self.message)?;
        }
        if self.code != 0 {
            let v = ErrorCode::from_i32(self.code)
                .ok_or_else(|| serde::ser::Error::custom(format!("Invalid variant {}", self.code)))?;
            struct_ser.serialize_field("code", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for KeystoreError {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "message",
            "code",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Message,
            Code,
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
                            "message" => Ok(GeneratedField::Message),
                            "code" => Ok(GeneratedField::Code),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = KeystoreError;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.KeystoreError")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<KeystoreError, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut message__ = None;
                let mut code__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Message => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("message"));
                            }
                            message__ = Some(map.next_value()?);
                        }
                        GeneratedField::Code => {
                            if code__.is_some() {
                                return Err(serde::de::Error::duplicate_field("code"));
                            }
                            code__ = Some(map.next_value::<ErrorCode>()? as i32);
                        }
                    }
                }
                Ok(KeystoreError {
                    message: message__.unwrap_or_default(),
                    code: code__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.KeystoreError", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SaveInvitesRequest {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.SaveInvitesRequest", len)?;
        if !self.requests.is_empty() {
            struct_ser.serialize_field("requests", &self.requests)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SaveInvitesRequest {
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
            type Value = SaveInvitesRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.SaveInvitesRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SaveInvitesRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut requests__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Requests => {
                            if requests__.is_some() {
                                return Err(serde::de::Error::duplicate_field("requests"));
                            }
                            requests__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(SaveInvitesRequest {
                    requests: requests__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.SaveInvitesRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for save_invites_request::Request {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.content_topic.is_empty() {
            len += 1;
        }
        if self.timestamp_ns != 0 {
            len += 1;
        }
        if !self.payload.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.SaveInvitesRequest.Request", len)?;
        if !self.content_topic.is_empty() {
            struct_ser.serialize_field("contentTopic", &self.content_topic)?;
        }
        if self.timestamp_ns != 0 {
            struct_ser.serialize_field("timestampNs", ToString::to_string(&self.timestamp_ns).as_str())?;
        }
        if !self.payload.is_empty() {
            struct_ser.serialize_field("payload", pbjson::private::base64::encode(&self.payload).as_str())?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for save_invites_request::Request {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "content_topic",
            "contentTopic",
            "timestamp_ns",
            "timestampNs",
            "payload",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ContentTopic,
            TimestampNs,
            Payload,
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
                            "contentTopic" | "content_topic" => Ok(GeneratedField::ContentTopic),
                            "timestampNs" | "timestamp_ns" => Ok(GeneratedField::TimestampNs),
                            "payload" => Ok(GeneratedField::Payload),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = save_invites_request::Request;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.SaveInvitesRequest.Request")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<save_invites_request::Request, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut content_topic__ = None;
                let mut timestamp_ns__ = None;
                let mut payload__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::ContentTopic => {
                            if content_topic__.is_some() {
                                return Err(serde::de::Error::duplicate_field("contentTopic"));
                            }
                            content_topic__ = Some(map.next_value()?);
                        }
                        GeneratedField::TimestampNs => {
                            if timestamp_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("timestampNs"));
                            }
                            timestamp_ns__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Payload => {
                            if payload__.is_some() {
                                return Err(serde::de::Error::duplicate_field("payload"));
                            }
                            payload__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                    }
                }
                Ok(save_invites_request::Request {
                    content_topic: content_topic__.unwrap_or_default(),
                    timestamp_ns: timestamp_ns__.unwrap_or_default(),
                    payload: payload__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.SaveInvitesRequest.Request", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SaveInvitesResponse {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.SaveInvitesResponse", len)?;
        if !self.responses.is_empty() {
            struct_ser.serialize_field("responses", &self.responses)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SaveInvitesResponse {
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
            type Value = SaveInvitesResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.SaveInvitesResponse")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SaveInvitesResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut responses__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Responses => {
                            if responses__.is_some() {
                                return Err(serde::de::Error::duplicate_field("responses"));
                            }
                            responses__ = Some(map.next_value()?);
                        }
                    }
                }
                Ok(SaveInvitesResponse {
                    responses: responses__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.SaveInvitesResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for save_invites_response::Response {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.response.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.SaveInvitesResponse.Response", len)?;
        if let Some(v) = self.response.as_ref() {
            match v {
                save_invites_response::response::Response::Result(v) => {
                    struct_ser.serialize_field("result", v)?;
                }
                save_invites_response::response::Response::Error(v) => {
                    struct_ser.serialize_field("error", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for save_invites_response::Response {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "result",
            "error",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Result,
            Error,
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
                            "result" => Ok(GeneratedField::Result),
                            "error" => Ok(GeneratedField::Error),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = save_invites_response::Response;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.SaveInvitesResponse.Response")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<save_invites_response::Response, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut response__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Result => {
                            if response__.is_some() {
                                return Err(serde::de::Error::duplicate_field("result"));
                            }
                            response__ = map.next_value::<::std::option::Option<_>>()?.map(save_invites_response::response::Response::Result)
;
                        }
                        GeneratedField::Error => {
                            if response__.is_some() {
                                return Err(serde::de::Error::duplicate_field("error"));
                            }
                            response__ = map.next_value::<::std::option::Option<_>>()?.map(save_invites_response::response::Response::Error)
;
                        }
                    }
                }
                Ok(save_invites_response::Response {
                    response: response__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.SaveInvitesResponse.Response", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for save_invites_response::response::Success {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.conversation.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.SaveInvitesResponse.Response.Success", len)?;
        if let Some(v) = self.conversation.as_ref() {
            struct_ser.serialize_field("conversation", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for save_invites_response::response::Success {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "conversation",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Conversation,
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
                            "conversation" => Ok(GeneratedField::Conversation),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = save_invites_response::response::Success;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.SaveInvitesResponse.Response.Success")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<save_invites_response::response::Success, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut conversation__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Conversation => {
                            if conversation__.is_some() {
                                return Err(serde::de::Error::duplicate_field("conversation"));
                            }
                            conversation__ = map.next_value()?;
                        }
                    }
                }
                Ok(save_invites_response::response::Success {
                    conversation: conversation__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.SaveInvitesResponse.Response.Success", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SignDigestRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.digest.is_empty() {
            len += 1;
        }
        if self.signer.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.SignDigestRequest", len)?;
        if !self.digest.is_empty() {
            struct_ser.serialize_field("digest", pbjson::private::base64::encode(&self.digest).as_str())?;
        }
        if let Some(v) = self.signer.as_ref() {
            match v {
                sign_digest_request::Signer::IdentityKey(v) => {
                    struct_ser.serialize_field("identityKey", v)?;
                }
                sign_digest_request::Signer::PrekeyIndex(v) => {
                    struct_ser.serialize_field("prekeyIndex", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SignDigestRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "digest",
            "identity_key",
            "identityKey",
            "prekey_index",
            "prekeyIndex",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Digest,
            IdentityKey,
            PrekeyIndex,
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
                            "digest" => Ok(GeneratedField::Digest),
                            "identityKey" | "identity_key" => Ok(GeneratedField::IdentityKey),
                            "prekeyIndex" | "prekey_index" => Ok(GeneratedField::PrekeyIndex),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SignDigestRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.SignDigestRequest")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<SignDigestRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut digest__ = None;
                let mut signer__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Digest => {
                            if digest__.is_some() {
                                return Err(serde::de::Error::duplicate_field("digest"));
                            }
                            digest__ = 
                                Some(map.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::IdentityKey => {
                            if signer__.is_some() {
                                return Err(serde::de::Error::duplicate_field("identityKey"));
                            }
                            signer__ = map.next_value::<::std::option::Option<_>>()?.map(sign_digest_request::Signer::IdentityKey);
                        }
                        GeneratedField::PrekeyIndex => {
                            if signer__.is_some() {
                                return Err(serde::de::Error::duplicate_field("prekeyIndex"));
                            }
                            signer__ = map.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| sign_digest_request::Signer::PrekeyIndex(x.0));
                        }
                    }
                }
                Ok(SignDigestRequest {
                    digest: digest__.unwrap_or_default(),
                    signer: signer__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.SignDigestRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TopicMap {
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
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.TopicMap", len)?;
        if !self.topics.is_empty() {
            struct_ser.serialize_field("topics", &self.topics)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TopicMap {
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
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TopicMap;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.TopicMap")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<TopicMap, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut topics__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::Topics => {
                            if topics__.is_some() {
                                return Err(serde::de::Error::duplicate_field("topics"));
                            }
                            topics__ = Some(
                                map.next_value::<std::collections::HashMap<_, _>>()?
                            );
                        }
                    }
                }
                Ok(TopicMap {
                    topics: topics__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.TopicMap", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for topic_map::TopicData {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.created_ns != 0 {
            len += 1;
        }
        if !self.peer_address.is_empty() {
            len += 1;
        }
        if self.invitation.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.keystore_api.v1.TopicMap.TopicData", len)?;
        if self.created_ns != 0 {
            struct_ser.serialize_field("createdNs", ToString::to_string(&self.created_ns).as_str())?;
        }
        if !self.peer_address.is_empty() {
            struct_ser.serialize_field("peerAddress", &self.peer_address)?;
        }
        if let Some(v) = self.invitation.as_ref() {
            struct_ser.serialize_field("invitation", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for topic_map::TopicData {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "created_ns",
            "createdNs",
            "peer_address",
            "peerAddress",
            "invitation",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            CreatedNs,
            PeerAddress,
            Invitation,
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
                            "createdNs" | "created_ns" => Ok(GeneratedField::CreatedNs),
                            "peerAddress" | "peer_address" => Ok(GeneratedField::PeerAddress),
                            "invitation" => Ok(GeneratedField::Invitation),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = topic_map::TopicData;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.keystore_api.v1.TopicMap.TopicData")
            }

            fn visit_map<V>(self, mut map: V) -> std::result::Result<topic_map::TopicData, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut created_ns__ = None;
                let mut peer_address__ = None;
                let mut invitation__ = None;
                while let Some(k) = map.next_key()? {
                    match k {
                        GeneratedField::CreatedNs => {
                            if created_ns__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createdNs"));
                            }
                            created_ns__ = 
                                Some(map.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::PeerAddress => {
                            if peer_address__.is_some() {
                                return Err(serde::de::Error::duplicate_field("peerAddress"));
                            }
                            peer_address__ = Some(map.next_value()?);
                        }
                        GeneratedField::Invitation => {
                            if invitation__.is_some() {
                                return Err(serde::de::Error::duplicate_field("invitation"));
                            }
                            invitation__ = map.next_value()?;
                        }
                    }
                }
                Ok(topic_map::TopicData {
                    created_ns: created_ns__.unwrap_or_default(),
                    peer_address: peer_address__.unwrap_or_default(),
                    invitation: invitation__,
                })
            }
        }
        deserializer.deserialize_struct("xmtp.keystore_api.v1.TopicMap.TopicData", FIELDS, GeneratedVisitor)
    }
}
