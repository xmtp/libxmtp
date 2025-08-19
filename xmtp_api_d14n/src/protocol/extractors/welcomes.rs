use xmtp_proto::{ConversionError, mls_v1};

use crate::protocol::Extractor;
use crate::protocol::traits::EnvelopeVisitor;
use xmtp_proto::mls_v1::welcome_message_input::V1 as WelcomeMessageV1;
use xmtp_proto::xmtp::xmtpv4::envelopes::UnsignedOriginatorEnvelope;

/// Type to extract a Welcome Message from Originator Envelopes
#[derive(Default)]
pub struct WelcomeMessageExtractor {
    originator_node_id: u32,
    originator_sequence_id: u64,
    created_ns: u64,
    welcome_message: mls_v1::WelcomeMessage,
}

impl Extractor for WelcomeMessageExtractor {
    type Output = mls_v1::WelcomeMessage;

    fn get(self) -> Self::Output {
        self.welcome_message
    }
}

impl EnvelopeVisitor<'_> for WelcomeMessageExtractor {
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

    fn visit_welcome_message_v1(&mut self, message: &WelcomeMessageV1) -> Result<(), Self::Error> {
        let message = mls_v1::welcome_message::Version::V1(mls_v1::welcome_message::V1 {
            id: self.originator_sequence_id,
            created_ns: self.created_ns,
            installation_key: message.installation_key.clone(),
            data: message.data.clone(),
            hpke_public_key: message.hpke_public_key.clone(),
            wrapper_algorithm: message.wrapper_algorithm,
            welcome_metadata: message.welcome_metadata.clone(),
        });
        self.welcome_message = mls_v1::WelcomeMessage {
            version: Some(message),
        };
        Ok(())
    }
}
