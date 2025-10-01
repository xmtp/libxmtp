use xmtp_common::RetryableError;

use crate::protocol::traits::EnvelopeVisitor;
use crate::protocol::{EnvelopeError, ExtractionError};
use xmtp_proto::xmtp::identity::associations::IdentityUpdate;
use xmtp_proto::xmtp::mls::api::v1::UploadKeyPackageRequest;
use xmtp_proto::xmtp::mls::api::v1::{GroupMessageInput, WelcomeMessageInput};
use xmtp_proto::xmtp::xmtpv4::envelopes::client_envelope::Payload;

/// Extract Topics from Envelopes
#[derive(Default, Clone, Debug)]
pub struct PayloadExtractor {
    payload: Option<Payload>,
}

impl PayloadExtractor {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn get(self) -> Result<Payload, PayloadExtractionError> {
        self.payload.ok_or(PayloadExtractionError::Failed)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum PayloadExtractionError {
    #[error("Failed to extract payload, wrong ProtocolMessage?")]
    Failed,
}

impl RetryableError for PayloadExtractionError {
    fn is_retryable(&self) -> bool {
        false
    }
}

impl From<PayloadExtractionError> for EnvelopeError {
    fn from(err: PayloadExtractionError) -> EnvelopeError {
        EnvelopeError::Extraction(ExtractionError::Payload(err))
    }
}

// TODO: at some point its possible to figure out how to borrow input
// from the Envelope and return it, but probably requires an entirely new
// 'accept_borrowed' path as well as some work to deal with the ::decode
// returning a newly allocated type. Not worth the effort yet.
impl EnvelopeVisitor<'_> for PayloadExtractor {
    type Error = PayloadExtractionError; // mostly is infallible
    fn visit_group_message_input(
        &mut self,
        message: &GroupMessageInput,
    ) -> Result<(), Self::Error> {
        tracing::debug!("Group Message Input");
        self.payload = Some(Payload::GroupMessage(message.clone()));
        Ok(())
    }

    fn visit_welcome_message_input(
        &mut self,
        message: &WelcomeMessageInput,
    ) -> Result<(), Self::Error> {
        self.payload = Some(Payload::WelcomeMessage(message.clone()));
        Ok(())
    }

    fn visit_upload_key_package(
        &mut self,
        kp: &UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        self.payload = Some(Payload::UploadKeyPackage(kp.clone()));
        Ok(())
    }

    fn visit_identity_update(&mut self, update: &IdentityUpdate) -> Result<(), Self::Error> {
        self.payload = Some(Payload::IdentityUpdate(update.clone()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ProtocolEnvelope;
    use crate::protocol::extractors::test_utils::*;
    use xmtp_proto::xmtp::xmtpv4::envelopes::client_envelope::Payload;

    #[xmtp_common::test]
    fn test_extract_group_message_payload() {
        let envelope = TestEnvelopeBuilder::new()
            .with_group_message_custom(vec![1, 2, 3], vec![4, 5, 6])
            .build();
        let mut extractor = PayloadExtractor::new();
        envelope.accept(&mut extractor).unwrap();
        let payload = extractor.get().unwrap();

        match payload {
            Payload::GroupMessage(msg) => {
                assert!(msg.version.is_some());
            }
            _ => panic!("Expected GroupMessage payload"),
        }
    }

    #[xmtp_common::test]
    fn test_extract_welcome_message_payload() {
        let envelope = TestEnvelopeBuilder::new()
            .with_welcome_message_detailed(vec![1, 2, 3], vec![4, 5, 6], vec![7, 8, 9])
            .build();
        let mut extractor = PayloadExtractor::new();
        envelope.accept(&mut extractor).unwrap();
        let payload = extractor.get().unwrap();

        match payload {
            Payload::WelcomeMessage(msg) => {
                assert!(msg.version.is_some());
            }
            _ => panic!("Expected WelcomeMessage payload"),
        }
    }

    #[xmtp_common::test]
    fn test_extract_key_package_payload() {
        let envelope = TestEnvelopeBuilder::new()
            .with_key_package_custom(vec![1, 2, 3])
            .build();
        let mut extractor = PayloadExtractor::new();
        envelope.accept(&mut extractor).unwrap();
        let payload = extractor.get().unwrap();

        match payload {
            Payload::UploadKeyPackage(kp) => {
                assert!(kp.key_package.is_some());
                assert!(!kp.is_inbox_id_credential);
            }
            _ => panic!("Expected UploadKeyPackage payload"),
        }
    }

    #[xmtp_common::test]
    fn test_extract_identity_update_payload() {
        let envelope = TestEnvelopeBuilder::new()
            .with_identity_update_custom("test_inbox".to_string())
            .build();
        let mut extractor = PayloadExtractor::new();
        envelope.accept(&mut extractor).unwrap();
        let payload = extractor.get().unwrap();

        match payload {
            Payload::IdentityUpdate(update) => {
                assert_eq!(update.inbox_id, "test_inbox");
            }
            _ => panic!("Expected IdentityUpdate payload"),
        }
    }

    #[xmtp_common::test]
    fn test_extract_no_payload_fails() {
        let envelope = TestEnvelopeBuilder::new().with_empty_payload().build();
        let mut extractor = PayloadExtractor::new();
        envelope.accept(&mut extractor).unwrap();
        let result = extractor.get();

        assert!(result.is_err());
        matches!(result.unwrap_err(), PayloadExtractionError::Failed);
    }
}
