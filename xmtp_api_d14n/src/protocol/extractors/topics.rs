use hex::FromHexError;
use openmls::framing::errors::ProtocolMessageError;
use xmtp_common::RetryableError;
use xmtp_proto::ConversionError;

use crate::protocol::ExtractionError;

use super::{EnvelopeError, Extractor};
use crate::protocol::{TopicKind, traits::EnvelopeVisitor};
use openmls::prelude::KeyPackageVerifyError;
use openmls::{
    framing::MlsMessageIn,
    prelude::{KeyPackageIn, ProtocolMessage, tls_codec::Deserialize},
};
use openmls_rust_crypto::RustCrypto;
use xmtp_proto::xmtp::identity::api::v1::get_identity_updates_request;
use xmtp_proto::xmtp::identity::associations::IdentityUpdate;
use xmtp_proto::xmtp::mls::api::v1::KeyPackageUpload;
use xmtp_proto::xmtp::mls::api::v1::UploadKeyPackageRequest;
use xmtp_proto::xmtp::mls::api::v1::{
    group_message_input::V1 as GroupMessageV1, welcome_message_input::V1 as WelcomeMessageV1,
};

/// Extract Topics from Envelopes
#[derive(Default, Clone, Debug)]
pub struct TopicExtractor {
    topic: Option<Vec<u8>>,
}

impl TopicExtractor {
    pub fn new() -> Self {
        Default::default()
    }
}
impl Extractor for TopicExtractor {
    type Output = Result<Vec<u8>, TopicExtractionError>;

    fn get(self) -> Self::Output {
        self.topic.ok_or(TopicExtractionError::Failed)
    }
}

impl TopicExtractor {
    pub fn get(self) -> Result<Vec<u8>, TopicExtractionError> {
        self.topic.ok_or(TopicExtractionError::Failed)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum TopicExtractionError {
    #[error("Topic extraction failed, no topic available")]
    Failed,
    #[error(transparent)]
    KeyPackageVerify(#[from] KeyPackageVerifyError),
    #[error(transparent)]
    Mls(#[from] openmls::prelude::Error),
    #[error(transparent)]
    Protocol(#[from] ProtocolMessageError),
    #[error(transparent)]
    FromHex(#[from] FromHexError),
    #[error(transparent)]
    Conversion(#[from] ConversionError),
}

impl RetryableError for TopicExtractionError {
    fn is_retryable(&self) -> bool {
        false
    }
}

impl From<TopicExtractionError> for EnvelopeError {
    fn from(err: TopicExtractionError) -> EnvelopeError {
        EnvelopeError::Extraction(ExtractionError::Topic(err))
    }
}

impl EnvelopeVisitor<'_> for TopicExtractor {
    type Error = TopicExtractionError;

    fn visit_group_message_v1(&mut self, message: &GroupMessageV1) -> Result<(), Self::Error> {
        let msg_result = MlsMessageIn::tls_deserialize(&mut message.data.as_slice())?;
        let protocol_message: ProtocolMessage = msg_result.try_into_protocol_message()?;
        self.topic = Some(TopicKind::GroupMessagesV1.build(protocol_message.group_id().as_slice()));
        Ok(())
    }

    fn visit_welcome_message_v1(&mut self, message: &WelcomeMessageV1) -> Result<(), Self::Error> {
        self.topic = Some(TopicKind::WelcomeMessagesV1.build(message.installation_key.as_slice()));
        Ok(())
    }

    fn visit_upload_key_package(
        &mut self,
        kp: &UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        let upload = kp.key_package.as_ref().ok_or(ConversionError::Missing {
            item: "key_package",
            r#type: std::any::type_name::<KeyPackageUpload>(),
        })?;
        let kp_in: KeyPackageIn =
            KeyPackageIn::tls_deserialize_exact(upload.key_package_tls_serialized.as_slice())?;
        let rust_crypto = RustCrypto::default();
        let kp = kp_in.validate(
            &rust_crypto,
            xmtp_configuration::MLS_PROTOCOL_VERSION,
            openmls::prelude::LeafNodeLifetimePolicy::Verify,
        )?;
        let installation_key = kp.leaf_node().signature_key().as_slice();
        self.topic = Some(TopicKind::KeyPackagesV1.build(installation_key));
        Ok(())
    }

    fn visit_identity_update(&mut self, update: &IdentityUpdate) -> Result<(), Self::Error> {
        let decoded_id = hex::decode(&update.inbox_id)?;
        self.topic = Some(TopicKind::IdentityUpdatesV1.build(&decoded_id));
        Ok(())
    }

    fn visit_identity_updates_request(
        &mut self,
        update: &get_identity_updates_request::Request,
    ) -> Result<(), Self::Error> {
        let decoded_id = hex::decode(&update.inbox_id)?;
        self.topic = Some(TopicKind::IdentityUpdatesV1.build(&decoded_id));
        Ok(())
    }
}
