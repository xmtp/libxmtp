use hex::FromHexError;
use openmls::framing::errors::ProtocolMessageError;
use xmtp_common::{RetryableError, retryable};
use xmtp_proto::identity_v1::get_identity_updates_response::IdentityUpdateLog;
use xmtp_proto::{ConversionError, mls_v1};

use super::{EnvelopeCollection, EnvelopeError, Extractor, ProtocolEnvelope};
use super::{TopicKind, traits::EnvelopeVisitor};
use openmls::prelude::KeyPackageVerifyError;
use openmls::{
    framing::MlsMessageIn,
    prelude::{KeyPackageIn, ProtocolMessage, tls_codec::Deserialize},
};
use openmls_rust_crypto::RustCrypto;
use std::marker::PhantomData;
use xmtp_proto::xmtp::identity::api::v1::get_identity_updates_request;
use xmtp_proto::xmtp::identity::associations::IdentityUpdate;
use xmtp_proto::xmtp::mls::api::v1::KeyPackageUpload;
use xmtp_proto::xmtp::mls::api::v1::UploadKeyPackageRequest;
use xmtp_proto::xmtp::mls::api::v1::fetch_key_packages_response::KeyPackage;
use xmtp_proto::xmtp::mls::api::v1::{GroupMessageInput, WelcomeMessageInput};
use xmtp_proto::xmtp::mls::api::v1::{
    group_message_input::V1 as GroupMessageV1, welcome_message_input::V1 as WelcomeMessageV1,
};
use xmtp_proto::xmtp::mls::message_contents::WelcomeWrapperAlgorithm;
use xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope;
use xmtp_proto::xmtp::xmtpv4::envelopes::UnsignedOriginatorEnvelope;
use xmtp_proto::xmtp::xmtpv4::envelopes::client_envelope::Payload;

/// Extract Data from a collection of envelopes
/// Does not preserve sequenced data, use [`SequencedExtractor`]
/// to preserve SequenceID/OriginatorID
/// runs with one extractor.
pub struct CollectionExtractor<Envelopes, Extractor> {
    envelopes: Envelopes,
    extractor: Extractor,
}

impl<Envelopes, Extractor> CollectionExtractor<Envelopes, Extractor> {
    pub fn new(envelopes: Envelopes, extractor: Extractor) -> Self {
        Self {
            envelopes,
            extractor,
        }
    }
}

impl<'a, Envelopes, E> Extractor for CollectionExtractor<Envelopes, E>
where
    Envelopes: EnvelopeCollection<'a> + IntoIterator,
    <Envelopes as IntoIterator>::Item: ProtocolEnvelope<'a>,
    E: Extractor + EnvelopeVisitor<'a>,
    EnvelopeError: From<<E as EnvelopeVisitor<'a>>::Error>,
{
    type Output = Result<<E as Extractor>::Output, EnvelopeError>;

    fn get(mut self) -> Self::Output {
        for envelope in self.envelopes.into_iter() {
            envelope.accept(&mut self.extractor)?;
        }
        Ok(self.extractor.get())
    }
}

/// Build a [`SequencedExtractor`]
#[derive(Default)]
pub struct SequencedExtractorBuilder<Envelope> {
    envelopes: Vec<Envelope>,
}

impl<Envelope> SequencedExtractorBuilder<Envelope> {
    pub fn envelopes<E>(self, envelopes: Vec<E>) -> SequencedExtractorBuilder<E> {
        SequencedExtractorBuilder::<E> { envelopes }
    }

    pub fn build<Extractor>(self) -> SequencedExtractor<Envelope, Extractor> {
        SequencedExtractor {
            envelopes: self.envelopes,
            _marker: PhantomData,
        }
    }
}

/// Extract data from a sequence of envelopes, preserving
/// per-envelope data like Sequence ID
// TODO: Could probably act on a generic of impl Iterator
// but could be a later improvement
pub struct SequencedExtractor<Envelope, Extractor> {
    envelopes: Vec<Envelope>,
    _marker: PhantomData<Extractor>,
}

impl SequencedExtractor<(), ()> {
    pub fn builder() -> SequencedExtractorBuilder<()> {
        SequencedExtractorBuilder::default()
    }
}

impl<'a, Envelope, E> Extractor for SequencedExtractor<Envelope, E>
where
    E: Extractor + EnvelopeVisitor<'a> + Default,
    Envelope: ProtocolEnvelope<'a>,
    EnvelopeError: From<<E as EnvelopeVisitor<'a>>::Error>,
{
    type Output = Result<Vec<<E as Extractor>::Output>, EnvelopeError>;

    fn get(self) -> Self::Output {
        let mut out = Vec::with_capacity(self.envelopes.len());
        for envelope in self.envelopes.into_iter() {
            let mut extractor = E::default();
            envelope.accept(&mut extractor)?;
            out.push(extractor.get());
        }
        Ok(out)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ExtractionError {
    #[error(transparent)]
    Payload(#[from] PayloadExtractionError),
    #[error(transparent)]
    Topic(#[from] TopicExtractionError),
}

impl RetryableError for ExtractionError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Payload(p) => retryable!(p),
            Self::Topic(t) => retryable!(t),
        }
    }
}

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
            wrapper_algorithm: WelcomeWrapperAlgorithm::Curve25519.into(),
        });
        self.welcome_message = mls_v1::WelcomeMessage {
            version: Some(message),
        };
        Ok(())
    }
}

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

/// Key Packages Extractor
/// This Extractor can be applied to multiple envelopes without losing state
#[derive(Default, Clone)]
pub struct KeyPackagesExtractor {
    key_packages: Vec<KeyPackage>,
}

impl Extractor for KeyPackagesExtractor {
    type Output = Vec<KeyPackage>;

    fn get(self) -> Self::Output {
        self.key_packages
    }
}

impl KeyPackagesExtractor {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn get(self) -> Vec<KeyPackage> {
        self.key_packages
    }
}

impl EnvelopeVisitor<'_> for KeyPackagesExtractor {
    type Error = ConversionError;

    fn visit_client(&mut self, e: &ClientEnvelope) -> Result<(), Self::Error> {
        tracing::debug!("client: {:?}", e);
        Ok(())
    }

    fn visit_none(&mut self) -> Result<(), Self::Error> {
        // TODO: Handle empty key package response (when key package is None)
        Ok(())
    }

    fn visit_upload_key_package(
        &mut self,
        req: &UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        let key_package = req.key_package.as_ref().ok_or(ConversionError::Missing {
            item: "key_package",
            r#type: "OriginatorEnvelope",
        })?;
        self.key_packages.push(KeyPackage {
            key_package_tls_serialized: key_package.key_package_tls_serialized.clone(),
        });
        Ok(())
    }
}

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
        let kp = kp_in.validate(&rust_crypto, super::MLS_PROTOCOL_VERSION)?;
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

/// Extract Identity Updates from Envelopes
#[derive(Default)]
pub struct IdentityUpdateExtractor {
    originator_node_id: u32,
    originator_sequence_id: u64,
    server_timestamp_ns: u64,
    update: IdentityUpdate,
}

impl Extractor for IdentityUpdateExtractor {
    type Output = (String, IdentityUpdateLog);

    fn get(self) -> Self::Output {
        (
            self.update.inbox_id.clone(),
            IdentityUpdateLog {
                sequence_id: self.originator_sequence_id,
                server_timestamp_ns: self.server_timestamp_ns,
                update: Some(self.update),
            },
        )
    }
}

/// extract an update from a single envelope
impl IdentityUpdateExtractor {
    pub fn new() -> Self {
        Self::default()
    }
}

impl EnvelopeVisitor<'_> for IdentityUpdateExtractor {
    type Error = PayloadExtractionError; // mostly is infallible

    fn visit_unsigned_originator(
        &mut self,
        envelope: &UnsignedOriginatorEnvelope,
    ) -> Result<(), Self::Error> {
        self.originator_node_id = envelope.originator_node_id;
        self.originator_sequence_id = envelope.originator_sequence_id;
        self.server_timestamp_ns = envelope.originator_ns as u64;
        Ok(())
    }

    fn visit_identity_update(&mut self, u: &IdentityUpdate) -> Result<(), Self::Error> {
        self.update = u.clone();
        Ok(())
    }
}

/*
pub struct EnvelopeValidator;
impl EnvelopeVisitor for EnvelopeValidator {
    fn visit_originator(&mut self, envelope: &OriginatorEnvelope) {
        todo!()
    }
}
*/
