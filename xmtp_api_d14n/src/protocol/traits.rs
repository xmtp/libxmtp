//! Traits to implement functionality according to
//! https://github.com/xmtp/XIPs/blob/main/XIPs/xip-49-decentralized-backend.md#33-client-to-node-protocol

use super::ExtractionError;
use super::PayloadExtractor;
use super::TopicExtractor;
use xmtp_common::RetryableError;
use xmtp_common::retryable;
use xmtp_proto::ConversionError;
use xmtp_proto::xmtp::identity::api::v1::get_identity_updates_request;
use xmtp_proto::xmtp::identity::associations::IdentityUpdate;
use xmtp_proto::xmtp::mls::api::v1::GroupMessageInput;
use xmtp_proto::xmtp::mls::api::v1::UploadKeyPackageRequest;
use xmtp_proto::xmtp::mls::api::v1::WelcomeMessageInput;
use xmtp_proto::xmtp::mls::api::v1::{
    group_message_input::{V1 as GroupMessageV1, Version as GroupMessageVersion},
    welcome_message_input::{V1 as WelcomeMessageV1, Version as WelcomeMessageVersion},
};
use xmtp_proto::xmtp::xmtpv4::envelopes::AuthenticatedData;
use xmtp_proto::xmtp::xmtpv4::envelopes::client_envelope::Payload;
use xmtp_proto::xmtp::xmtpv4::envelopes::{
    ClientEnvelope, OriginatorEnvelope, PayerEnvelope, UnsignedOriginatorEnvelope,
};
use xmtp_proto::xmtp::xmtpv4::message_api::get_newest_envelope_response;

/// Envelope Visitor type for ergonomic handling of serialized nested envelope types.
///
/// The blanket implementation on Vec<T> enables combining an arbitrary number of visitors into one,
///
/// process = vec![ValidateMessage::new(), ExtractMessage::new()];
/// Each step is ran in sequence, and if one of the steps fail, the entire process is
/// short-circuited.
/// This has the advantage of not re-doing deserialization for each processing step.
///
// NOTE: A new type wrapping Vec<T> can be created in order to avoid short-circuiting if that is
// desired.
pub trait EnvelopeVisitor<'env> {
    type Error: Into<EnvelopeError>;

    /// Visit the OriginatorEnvelope Type
    fn visit_originator(&mut self, _e: &OriginatorEnvelope) -> Result<(), Self::Error> {
        tracing::debug!("visit_originator");
        Ok(())
    }
    /// Visit the UnsignedOriginatorEnvelope type
    fn visit_unsigned_originator(
        &mut self,
        _e: &UnsignedOriginatorEnvelope,
    ) -> Result<(), Self::Error> {
        tracing::debug!("visit_unsigned_originator");
        Ok(())
    }
    /// Visit the Payer Envelope Type
    fn visit_payer(&mut self, _e: &PayerEnvelope) -> Result<(), Self::Error> {
        tracing::debug!("visit_payer");
        Ok(())
    }
    /// Visit the ClientEnvelope type
    fn visit_client(&mut self, _e: &ClientEnvelope) -> Result<(), Self::Error> {
        tracing::debug!("visit_client");
        Ok(())
    }
    /// Visit the GroupMessageInput type
    fn visit_group_message_version(&mut self, _m: &GroupMessageVersion) -> Result<(), Self::Error> {
        tracing::debug!("visit_group_message_version");
        Ok(())
    }
    /// Visit the WelcomeMessageInput containing the welcome message version
    fn visit_group_message_input(&mut self, _m: &GroupMessageInput) -> Result<(), Self::Error> {
        tracing::debug!("visit_group_message_input");
        Ok(())
    }

    /// Visit a V1 Group Message
    fn visit_group_message_v1(&mut self, _m: &GroupMessageV1) -> Result<(), Self::Error> {
        tracing::debug!("visit_group_message_v1");
        Ok(())
    }
    /// Visit the WelcomeMessageInput containing the welcome message version
    fn visit_welcome_message_version(
        &mut self,
        _m: &WelcomeMessageVersion,
    ) -> Result<(), Self::Error> {
        tracing::debug!("visit_group_message_version");
        Ok(())
    }
    /// Visit the WelcomeMessageInput containing the welcome message version
    fn visit_welcome_message_input(&mut self, _m: &WelcomeMessageInput) -> Result<(), Self::Error> {
        tracing::debug!("visit_welcome_message_input");
        Ok(())
    }

    /// Visit a V1 Welcome Message
    fn visit_welcome_message_v1(&mut self, _m: &WelcomeMessageV1) -> Result<(), Self::Error> {
        tracing::debug!("visit_welcome_message_v1");
        Ok(())
    }
    /// Visit the Upload Key Package
    fn visit_upload_key_package(
        &mut self,
        _p: &UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        tracing::debug!("visit_upload_key_package");
        Ok(())
    }

    /// Visit the Identity Update Type
    fn visit_identity_update(&mut self, _u: &IdentityUpdate) -> Result<(), Self::Error> {
        tracing::debug!("visit_identity_update");
        Ok(())
    }

    /// Visit an Identity Updates Request
    fn visit_identity_updates_request(
        &mut self,
        _u: &get_identity_updates_request::Request,
    ) -> Result<(), Self::Error> {
        tracing::debug!("visit_identity_updates_request");
        Ok(())
    }

    /// Visit an empty type in a fixed-length array
    /// Useful is client expects a constant length between
    /// requests and responses
    fn visit_none(&mut self) -> Result<(), Self::Error> {
        tracing::debug!("visit_none");
        Ok(())
    }

    /// Visit a Newest Envelope Response
    fn visit_newest_envelope_response(
        &mut self,
        _u: &get_newest_envelope_response::Response,
    ) -> Result<(), Self::Error> {
        tracing::debug!("visit_newest_envelope_response");
        Ok(())
    }
}

pub type VisitError<'a, V> = <V as EnvelopeVisitor<'a>>::Error;

/// An low-level envelope from the network gRPC interface
pub trait ProtocolEnvelope<'env> {
    type Nested<'a>
    where
        Self: 'a;
    fn accept<V: EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>;
    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError>;
}

#[derive(thiserror::Error, Debug)]
pub enum EnvelopeError {
    #[error(transparent)]
    Conversion(#[from] ConversionError),
    #[error(transparent)]
    Extraction(#[from] ExtractionError),
    #[error("Each topic must have a payload")]
    TopicMismatch,
    // for extractors defined outside of this crate or
    // generic implementations like Tuples
    #[error("{0}")]
    DynError(Box<dyn RetryableError + Send + Sync>),
}

impl RetryableError for EnvelopeError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Conversion(c) => retryable!(c),
            Self::Extraction(e) => retryable!(e),
            Self::TopicMismatch => false,
            Self::DynError(d) => retryable!(d),
        }
    }
}

// TODO: Make topic type instead of bytes
// topic not fixed length
/// A Generic Higher-Level Envelope
pub trait Envelope<'env> {
    /// Get the topic for an envelope
    fn topics(&self) -> Result<Vec<Vec<u8>>, EnvelopeError>;
    /// Get the payload for an envelope
    fn payload(&self) -> Result<Vec<Payload>, EnvelopeError>;
    /// Build the ClientEnvelope
    fn client_envelopes(&self) -> Result<Vec<ClientEnvelope>, EnvelopeError>;
}

/// Allows us to call these methods straight on the protobuf types without any
/// parsing/matching first.
impl<'env, T> Envelope<'env> for T
where
    T: ProtocolEnvelope<'env> + std::fmt::Debug,
{
    fn topics(&self) -> Result<Vec<Vec<u8>>, EnvelopeError> {
        let mut extractor = TopicExtractor::new();
        self.accept(&mut extractor)?;
        Ok(extractor.get()?)
    }

    fn payload(&self) -> Result<Vec<Payload>, EnvelopeError> {
        let mut extractor = PayloadExtractor::new();
        self.accept(&mut extractor)?;
        Ok(extractor.get()?)
    }

    fn client_envelopes(&self) -> Result<Vec<ClientEnvelope>, EnvelopeError> {
        // ensures we only recurse the proto data structure once.
        let mut extractor = (TopicExtractor::new(), PayloadExtractor::new());
        self.accept(&mut extractor)?;
        let topic = extractor.0.get().map_err(ExtractionError::from)?;
        let payload = extractor.1.get().map_err(ExtractionError::from)?;
        // this should never happen since Self is the same type, but the error is defensive
        if topic.len() != payload.len() {
            return Err(EnvelopeError::TopicMismatch);
        }
        Ok(topic
            .into_iter()
            .zip(payload)
            .map(|(topic, payload)| ClientEnvelope {
                aad: Some(AuthenticatedData::with_topic(topic)),
                payload: Some(payload),
            })
            .collect::<Vec<ClientEnvelope>>())
    }
}

/// Sort
pub trait Sort {
    /// Sort envelopes by timestamp in-place
    fn timestamp_sort(&mut self);
    /// Casually Sort envelopes in-place
    fn casual_sort(&mut self, topic_cursor: usize);
}

/*
impl Sort for Vec<Envelope> {
    fn timestamp_sort(&mut self) {
        todo!()
    }

    fn casual_sort(&mut self, topic_cursor: usize) {
        todo!()
    }
}
*/
