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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ProtocolEnvelope;
    use crate::protocol::extractors::test_utils::*;

    #[xmtp_common::test]
    fn test_extract_welcome_message() {
        let installation_key = xmtp_common::rand_vec::<32>();
        let data = xmtp_common::rand_vec::<64>();
        let hpke_public_key = xmtp_common::rand_vec::<32>();

        let envelope = TestEnvelopeBuilder::new()
            .with_originator_node_id(123)
            .with_originator_sequence_id(456)
            .with_originator_ns(789)
            .with_welcome_message_full(
                installation_key.clone(),
                data.clone(),
                hpke_public_key.clone(),
                1,
                vec![1, 2, 3],
            )
            .build();

        let mut extractor = WelcomeMessageExtractor::default();
        envelope.accept(&mut extractor).unwrap();
        let welcome_message = extractor.get();

        let version = welcome_message.version.unwrap();
        match version {
            mls_v1::welcome_message::Version::V1(v1) => {
                assert_eq!(v1.id, 456);
                assert_eq!(v1.created_ns, 789);
                assert_eq!(v1.installation_key, installation_key);
                assert_eq!(v1.data, data);
                assert_eq!(v1.hpke_public_key, hpke_public_key);
                assert_eq!(v1.wrapper_algorithm, 1);
                assert_eq!(v1.welcome_metadata, vec![1, 2, 3]);
            }
            mls_v1::welcome_message::Version::WelcomePointer(_) => {
                unimplemented!("WelcomePointer not supported");
            }
        }
    }
}
