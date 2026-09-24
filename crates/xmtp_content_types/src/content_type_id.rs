use xmtp_proto::xmtp::mls::message_contents::ContentTypeId as ProtoContentTypeId;

/// The three values used to match a content codec.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ContentTypeId {
    pub authority_id: String,
    pub type_id: String,
    pub version_major: u32,
}

impl From<ProtoContentTypeId> for ContentTypeId {
    fn from(value: ProtoContentTypeId) -> Self {
        Self {
            authority_id: value.authority_id,
            type_id: value.type_id,
            version_major: value.version_major,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[xmtp_common::test]
    fn proto_minor_version_does_not_change_codec_id() {
        let proto = ProtoContentTypeId {
            authority_id: "xmtp.org".into(),
            type_id: "text".into(),
            version_major: 1,
            version_minor: 4,
        };
        let id = ContentTypeId::from(proto.clone());
        assert_eq!(
            id,
            ContentTypeId::from(ProtoContentTypeId {
                version_minor: 9,
                ..proto
            })
        );
    }
}
