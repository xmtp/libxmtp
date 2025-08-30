use xmtp_cryptography::hash::sha256_bytes;
use xmtp_proto::{
    ConversionError,
    mls_v1::group_message,
    types::{Cursor, GroupMessage, GroupMessageBuilder},
};

use crate::protocol::traits::EnvelopeVisitor;
use crate::protocol::{ExtractionError, Extractor};
use chrono::DateTime;
use openmls::{
    framing::MlsMessageIn,
    prelude::{ContentType, ProtocolMessage, tls_codec::Deserialize},
};
use xmtp_proto::xmtp::mls::api::v1::group_message_input;
use xmtp_proto::xmtp::xmtpv4::envelopes::UnsignedOriginatorEnvelope;

/// Type to extract a Group Message from Originator Envelopes
#[derive(Default)]
pub struct GroupMessageExtractor {
    group_message: GroupMessageBuilder,
}

impl Extractor for GroupMessageExtractor {
    type Output = Result<GroupMessage, ExtractionError>;

    fn get(self) -> Self::Output {
        Ok(self.group_message.build()?)
    }
}

impl EnvelopeVisitor<'_> for GroupMessageExtractor {
    type Error = ConversionError;

    fn visit_unsigned_originator(
        &mut self,
        envelope: &UnsignedOriginatorEnvelope,
    ) -> Result<(), Self::Error> {
        self.group_message
            .created_ns(DateTime::from_timestamp_nanos(envelope.originator_ns))
            .cursor(Cursor {
                originator_id: envelope.originator_node_id,
                sequence_id: envelope.originator_sequence_id,
            });
        Ok(())
    }

    fn visit_group_message_v1(
        &mut self,
        message: &group_message_input::V1,
    ) -> Result<(), Self::Error> {
        let payload_hash = sha256_bytes(message.data.as_slice());
        self.group_message
            .sender_hmac(message.sender_hmac.clone())
            .should_push(message.should_push)
            .payload_hash(payload_hash);
        extract_common_mls(&mut self.group_message, &message.data)?;
        Ok(())
    }
}

#[derive(Default)]
pub struct V3GroupMessageExtractor {
    group_message: Option<GroupMessageBuilder>,
}

impl Extractor for V3GroupMessageExtractor {
    type Output = Result<Option<GroupMessage>, ConversionError>;

    fn get(self) -> Self::Output {
        if let Some(gm) = self.group_message {
            Ok(Some(gm.build()?))
        } else {
            Ok(None)
        }
    }
}

impl EnvelopeVisitor<'_> for V3GroupMessageExtractor {
    type Error = ConversionError;

    fn visit_v3_group_message(&mut self, message: &group_message::V1) -> Result<(), Self::Error> {
        let mut group_message = GroupMessage::builder();
        let payload_hash = sha256_bytes(message.data.as_slice());
        let is_commit = extract_common_mls(&mut group_message, &message.data)?;
        let originator_node_id = if is_commit {
            xmtp_configuration::Originators::MLS_COMMITS
        } else {
            xmtp_configuration::Originators::APPLICATION_MESSAGES
        };
        group_message
            .cursor(Cursor {
                originator_id: originator_node_id.into(),
                sequence_id: message.id,
            })
            .created_ns(DateTime::from_timestamp_nanos(message.created_ns as i64))
            .sender_hmac(message.sender_hmac.clone())
            .should_push(message.should_push)
            .payload_hash(payload_hash);

        self.group_message = Some(group_message);
        Ok(())
    }
}

/// extract common mls config
/// returns true if it is a commit
fn extract_common_mls(
    builder: &mut GroupMessageBuilder,
    mut data: &[u8],
) -> Result<bool, ConversionError> {
    let msg_in = MlsMessageIn::tls_deserialize(&mut data)?;
    let protocol_message: ProtocolMessage = msg_in.try_into_protocol_message()?;
    let is_commit = protocol_message.content_type() == ContentType::Commit;

    builder
        .group_id(protocol_message.group_id().to_vec())
        .message(protocol_message);
    Ok(is_commit)
}
