// @generated
impl serde::Serialize for Reaction {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.reference.is_empty() {
            len += 1;
        }
        if !self.reference_inbox_id.is_empty() {
            len += 1;
        }
        if !self.action.is_empty() {
            len += 1;
        }
        if !self.content.is_empty() {
            len += 1;
        }
        if !self.schema.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("xmtp.reactions.Reaction", len)?;
        if !self.reference.is_empty() {
            struct_ser.serialize_field("reference", &self.reference)?;
        }
        if !self.reference_inbox_id.is_empty() {
            struct_ser.serialize_field("referenceInboxId", &self.reference_inbox_id)?;
        }
        if !self.action.is_empty() {
            struct_ser.serialize_field("action", &self.action)?;
        }
        if !self.content.is_empty() {
            struct_ser.serialize_field("content", &self.content)?;
        }
        if !self.schema.is_empty() {
            struct_ser.serialize_field("schema", &self.schema)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for Reaction {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "reference",
            "reference_inbox_id",
            "referenceInboxId",
            "action",
            "content",
            "schema",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Reference,
            ReferenceInboxId,
            Action,
            Content,
            Schema,
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
                            "reference" => Ok(GeneratedField::Reference),
                            "referenceInboxId" | "reference_inbox_id" => Ok(GeneratedField::ReferenceInboxId),
                            "action" => Ok(GeneratedField::Action),
                            "content" => Ok(GeneratedField::Content),
                            "schema" => Ok(GeneratedField::Schema),
                            _ => Err(serde::de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = Reaction;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct xmtp.reactions.Reaction")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<Reaction, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut reference__ = None;
                let mut reference_inbox_id__ = None;
                let mut action__ = None;
                let mut content__ = None;
                let mut schema__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Reference => {
                            if reference__.is_some() {
                                return Err(serde::de::Error::duplicate_field("reference"));
                            }
                            reference__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ReferenceInboxId => {
                            if reference_inbox_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("referenceInboxId"));
                            }
                            reference_inbox_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Action => {
                            if action__.is_some() {
                                return Err(serde::de::Error::duplicate_field("action"));
                            }
                            action__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Content => {
                            if content__.is_some() {
                                return Err(serde::de::Error::duplicate_field("content"));
                            }
                            content__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Schema => {
                            if schema__.is_some() {
                                return Err(serde::de::Error::duplicate_field("schema"));
                            }
                            schema__ = Some(map_.next_value()?);
                        }
                    }
                }
                Ok(Reaction {
                    reference: reference__.unwrap_or_default(),
                    reference_inbox_id: reference_inbox_id__.unwrap_or_default(),
                    action: action__.unwrap_or_default(),
                    content: content__.unwrap_or_default(),
                    schema: schema__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("xmtp.reactions.Reaction", FIELDS, GeneratedVisitor)
    }
}
