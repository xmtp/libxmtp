//! Traits to implement functionality according to
//! https://github.com/xmtp/XIPs/blob/main/XIPs/xip-49-decentralized-backend.md#33-client-to-node-protocol

use super::ExtractionError;
use super::PayloadExtractor;
use super::TopicExtractor;
use super::ValidationError;
use xmtp_common::RetryableError;
use xmtp_common::retryable;
use xmtp_proto::ConversionError;
use xmtp_proto::xmtp::xmtpv4::envelopes::AuthenticatedData;
use xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope;
use xmtp_proto::xmtp::xmtpv4::envelopes::client_envelope::Payload;

mod visitor;
pub use visitor::*;

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
    #[error(transparent)]
    Validation(#[from] ValidationError),
}

impl RetryableError for EnvelopeError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Conversion(c) => retryable!(c),
            Self::Extraction(e) => retryable!(e),
            Self::TopicMismatch => false,
            Self::DynError(d) => retryable!(d),
            Self::Validation(e) => retryable!(e),
        }
    }
}

// TODO: Maybe we can do something with Iterators and Extractors
// to avoid the Combinators like `CollectionExtractor`?
// TODO: Make topic type instead of bytes
// topic not fixed length
/// A Generic Higher-Level Collection of Envelopes
pub trait EnvelopeCollection<'env> {
    /// Get the topic for an envelope
    fn topics(&self) -> Result<Vec<Vec<u8>>, EnvelopeError>;
    /// Get the payload for an envelope
    fn payloads(&self) -> Result<Vec<Payload>, EnvelopeError>;
    /// Build the ClientEnvelope
    fn client_envelopes(&self) -> Result<Vec<ClientEnvelope>, EnvelopeError>;
    /// Length of the Collection
    fn len(&self) -> usize;
    /// Whether the Collection of Envelopes is empty
    fn is_empty(&self) -> bool;
}

/// Represents a Single High-Level Envelope
pub trait Envelope<'env> {
    fn topic(&self) -> Result<Vec<u8>, EnvelopeError>;
    fn payload(&self) -> Result<Payload, EnvelopeError>;
    fn client_envelope(&self) -> Result<ClientEnvelope, EnvelopeError>;
}

pub trait Extractor {
    type Output;
    fn get(self) -> Self::Output;
}

/// A
/// llows us to call these methods straight on the protobuf types without any
/// parsing/matching first.
impl<'env, T> Envelope<'env> for T
where
    T: ProtocolEnvelope<'env> + std::fmt::Debug,
{
    fn topic(&self) -> Result<Vec<u8>, EnvelopeError> {
        let mut extractor = TopicExtractor::new();
        self.accept(&mut extractor)?;
        Ok(extractor.get()?)
    }

    fn payload(&self) -> Result<Payload, EnvelopeError> {
        let mut extractor = PayloadExtractor::new();
        self.accept(&mut extractor)?;
        Ok(extractor.get()?)
    }

    fn client_envelope(&self) -> Result<ClientEnvelope, EnvelopeError> {
        // ensures we only recurse the proto data structure once.
        let mut extractor = (TopicExtractor::new(), PayloadExtractor::new());
        self.accept(&mut extractor)?;
        let topic = extractor.0.get().map_err(ExtractionError::from)?;
        let payload = extractor.1.get().map_err(ExtractionError::from)?;
        Ok(ClientEnvelope {
            aad: Some(AuthenticatedData::with_topic(topic)),
            payload: Some(payload),
        })
    }
}

impl<'env, T> EnvelopeCollection<'env> for Vec<T>
where
    T: ProtocolEnvelope<'env> + std::fmt::Debug,
{
    fn topics(&self) -> Result<Vec<Vec<u8>>, EnvelopeError> {
        self.iter()
            .map(|t| t.topic())
            .collect::<Result<Vec<Vec<u8>>, _>>()
    }

    fn payloads(&self) -> Result<Vec<Payload>, EnvelopeError> {
        self.iter()
            .map(|t| t.payload())
            .collect::<Result<Vec<Payload>, _>>()
    }

    fn client_envelopes(&self) -> Result<Vec<ClientEnvelope>, EnvelopeError> {
        self.iter()
            .map(|t| t.client_envelope())
            .collect::<Result<Vec<ClientEnvelope>, EnvelopeError>>()
    }

    fn len(&self) -> usize {
        Vec::len(self)
    }

    fn is_empty(&self) -> bool {
        Vec::is_empty(self)
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
