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
