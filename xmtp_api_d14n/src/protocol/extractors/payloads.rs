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
    use crate::protocol::Envelope;
    use crate::protocol::extractors::test_utils::*;
    use xmtp_proto::mls_v1::{group_message_input, welcome_message_input};
    use xmtp_proto::xmtp::xmtpv4::envelopes::client_envelope::Payload;

    #[xmtp_common::test]
    fn test_extract_group_message_payload() {
        let envelope = TestEnvelopeBuilder::new()
            .with_group_message_custom(vec![1, 2, 3], vec![4, 5, 6])
            .build();
        let payload = envelope.payload().unwrap();

        match payload {
            Payload::GroupMessage(msg) => {
                assert!(msg.version.is_some());
                let m = msg.version.unwrap();
                let group_message_input::Version::V1(group_message_input::V1 { data, .. }) = m;
                assert_eq!(data, vec![1, 2, 3]);
            }
            _ => panic!("Expected GroupMessage payload"),
        }
    }

    #[xmtp_common::test]
    fn test_extract_welcome_message_payload() {
        let envelope = TestEnvelopeBuilder::new()
            .with_welcome_message(vec![1, 2, 3])
            .build();
        let payload = envelope.payload().unwrap();

        match payload {
            Payload::WelcomeMessage(msg) => {
                assert!(msg.version.is_some());
                let m = msg.version.unwrap();
                let welcome_message_input::Version::V1(welcome_message_input::V1 {
                    installation_key,
                    ..
                }) = m
                else {
                    unimplemented!("Expected WelcomeMessageV1 payload, got {:?}", m);
                };
                assert_eq!(installation_key, vec![1, 2, 3]);
            }
            _ => panic!("Expected WelcomeMessage payload"),
        }
    }

    #[xmtp_common::test]
    fn test_extract_key_package_payload() {
        let envelope = TestEnvelopeBuilder::new()
            .with_key_package_custom(vec![1, 2, 3])
            .build();
        let payload = envelope.payload().unwrap();

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
        let payload = envelope.payload().unwrap();

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
        let result = envelope.payload();
        assert!(result.is_err());
        matches!(
            result.unwrap_err(),
            EnvelopeError::Extraction(ExtractionError::Payload(PayloadExtractionError::Failed))
        );
    }
}
