use chrono::DateTime;
use xmtp_proto::{
    mls_v1::group_message::Version,
    types::{Cursor, GroupId, GroupMessageMetadata},
};

use crate::protocol::{
    EnvelopeError, EnvelopeVisitor, Extractor, GroupMessageExtractor, ProtocolEnvelope,
};

/// Extractor for converting GetNewestEnvelopeResponse results to GroupMessage responses
#[derive(Default, Clone)]
pub struct MessageMetadataExtractor {
    metadata: Vec<Option<GroupMessageMetadata>>,
}

impl MessageMetadataExtractor {
    pub fn new() -> Self {
        Default::default()
    }
}

impl Extractor for MessageMetadataExtractor {
    type Output = Vec<Option<GroupMessageMetadata>>;

    fn get(self) -> Self::Output {
        self.metadata
    }
}

impl EnvelopeVisitor<'_> for MessageMetadataExtractor {
    type Error = EnvelopeError;

    fn visit_newest_envelope_response(
        &mut self,
        response: &xmtp_proto::xmtp::xmtpv4::message_api::get_newest_envelope_response::Response,
    ) -> Result<(), Self::Error> {
        let message_metadata = if let Some(envelope) = &response.originator_envelope {
            let mut extractor = GroupMessageExtractor::default();
            envelope.accept(&mut extractor)?;
            let group_message = extractor.get()?;
            Some(
                GroupMessageMetadata::builder()
                    .created_ns(group_message.created_ns)
                    .cursor(group_message.cursor)
                    .group_id(group_message.group_id)
                    .build()?,
            )
        } else {
            None
        };

        self.metadata.push(message_metadata);

        Ok(())
    }

    fn visit_v3_group_message(
        &mut self,
        v1_message: &xmtp_proto::xmtp::mls::api::v1::group_message::V1,
    ) -> Result<(), Self::Error> {
        let cursor = if v1_message.is_commit {
            Cursor::mls_commits(v1_message.id)
        } else {
            Cursor::v3_messages(v1_message.id)
        };

        let group_id: GroupId = v1_message.group_id.clone().into();

        let metadata = GroupMessageMetadata::builder()
            .created_ns(DateTime::from_timestamp_nanos(v1_message.created_ns as i64))
            .cursor(cursor)
            .group_id(group_id)
            .build()?;

        self.metadata.push(Some(metadata));

        Ok(())
    }

    fn visit_newest_group_message_response(
        &mut self,
        response: &xmtp_proto::xmtp::mls::api::v1::get_newest_group_message_response::Response,
    ) -> Result<(), Self::Error> {
        if let Some(xmtp_proto::mls_v1::GroupMessage {
            version: Some(Version::V1(v1_message)),
        }) = &response.group_message
        {
            self.visit_v3_group_message(v1_message)?
        } else {
            self.metadata.push(None)
        }

        Ok(())
    }
}
