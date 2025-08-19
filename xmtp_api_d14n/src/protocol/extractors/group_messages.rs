use xmtp_proto::{ConversionError, mls_v1};

use crate::protocol::Extractor;
use crate::protocol::traits::EnvelopeVisitor;
use openmls::{
    framing::MlsMessageIn,
    prelude::{ProtocolMessage, tls_codec::Deserialize},
};
use xmtp_proto::xmtp::mls::api::v1::group_message_input::V1 as GroupMessageV1;
use xmtp_proto::xmtp::xmtpv4::envelopes::UnsignedOriginatorEnvelope;

/// Type to extract a Group Message from Originator Envelopes
#[derive(Default)]
pub struct GroupMessageExtractor {
    originator_node_id: u32,
    originator_sequence_id: u64,
    created_ns: u64,
    group_message: mls_v1::GroupMessage,
}

impl Extractor for GroupMessageExtractor {
    type Output = mls_v1::GroupMessage;

    fn get(self) -> Self::Output {
        self.group_message
    }
}

impl EnvelopeVisitor<'_> for GroupMessageExtractor {
    type Error = ConversionError;

    fn visit_unsigned_originator(
        &mut self,
        envelope: &UnsignedOriginatorEnvelope,
    ) -> Result<(), Self::Error> {
        self.originator_node_id = envelope.originator_node_id;
        self.originator_sequence_id = envelope.originator_sequence_id;
        self.created_ns = envelope.originator_ns as u64;
        Ok(())
    }

    fn visit_group_message_v1(&mut self, message: &GroupMessageV1) -> Result<(), Self::Error> {
        let msg_in = MlsMessageIn::tls_deserialize(&mut message.data.as_slice())?;
        let protocol_message: ProtocolMessage = msg_in.try_into_protocol_message()?;

        // TODO:insipx: we could easily extract more information here to make
        // processing messages easier
        // for instance, we have the epoch, group_id and data, and we can create
        // a XmtpGroupMessage struct to store this extra data rather than re-do deserialization
        // in 'process_message'
        // We can do that for v3 as well
        let message = mls_v1::group_message::Version::V1(mls_v1::group_message::V1 {
            id: self.originator_sequence_id,
            created_ns: self.created_ns,
            group_id: protocol_message.group_id().to_vec(),
            data: message.data.clone(),
            sender_hmac: message.sender_hmac.clone(),
            should_push: message.should_push,
        });
        self.group_message = mls_v1::GroupMessage {
            version: Some(message),
        };
        Ok(())
    }
}
