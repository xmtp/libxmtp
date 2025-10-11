//! Traits to implement functionality according to
//! <https://github.com/xmtp/XIPs/blob/main/XIPs/xip-49-decentralized-backend.md#33-client-to-node-protocol>

use crate::protocol::SequencedExtractor;

use super::ExtractionError;
use super::PayloadExtractor;
use super::TopicExtractor;
use xmtp_common::RetryableError;
use xmtp_common::retryable;
use xmtp_proto::ConversionError;
use xmtp_proto::xmtp::xmtpv4::envelopes::AuthenticatedData;
use xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope;
use xmtp_proto::xmtp::xmtpv4::envelopes::client_envelope::Payload;

mod visitor;
pub use visitor::*;

/// An low-level envelope from the network gRPC interface
/*
* WARN: ProtocolEnvelope implementation for a Vec<T>
* should be avoided, since it may cause Envelope
* to implicity act on a collection when a single envelope is expected.
* Theres a way to seal this trait implementation to
* avoid external implementations which should be done.
*/
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
    #[error("Envelope not found")]
    NotFound(&'static str),
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
            Self::NotFound(_) => false,
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
    /// run a sequenced extraction on the envelopes in this collection
    fn consume<E>(self) -> Result<Vec<<E as Extractor>::Output>, EnvelopeError>
    where
        for<'a> E: Default + Extractor + EnvelopeVisitor<'a>,
        for<'a> EnvelopeError: From<<E as EnvelopeVisitor<'a>>::Error>,
        Self: Sized;
}

/// Extension trait for an envelope collection which handles errors.
pub trait TryEnvelopeCollectionExt<'env>: EnvelopeCollection<'env> {
    /// run a sequenced extraction on the envelopes in this collection.
    /// Flattens and returns errors into one Result<_, E>
    fn try_consume<E>(self) -> Result<Vec<<E as TryExtractor>::Ok>, EnvelopeError>
    where
        for<'a> E: TryExtractor,
        for<'a> E: Default + EnvelopeVisitor<'a>,
        for<'a> EnvelopeError: From<<E as EnvelopeVisitor<'a>>::Error>,
        EnvelopeError: From<<E as TryExtractor>::Error>,
        Self: Sized,
    {
        Ok(self
            .consume::<E>()?
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?)
    }
    // TODO: fn like try_consume but that does not fail on only one element failure
    // i.e keeps processing messages, keeping errors around
}

impl<'env, T> TryEnvelopeCollectionExt<'env> for T where T: EnvelopeCollection<'env> {}

/// Represents a Single High-Level Envelope
pub trait Envelope<'env> {
    /// Extract the topic for this envelope
    fn topic(&self) -> Result<Vec<u8>, EnvelopeError>;
    /// Extract the payload for this envelope
    fn payload(&self) -> Result<Payload, EnvelopeError>;
    /// Extract the client envelope (envelope containing message payload & AAD, if any) for this
    /// envelope.
    fn client_envelope(&self) -> Result<ClientEnvelope, EnvelopeError>;
    /// consume this envelope by extracting its contents with extractor `E`
    fn consume<E>(&self, extractor: E) -> Result<E::Output, EnvelopeError>
    where
        Self: Sized,
        for<'a> EnvelopeError: From<<E as EnvelopeVisitor<'a>>::Error>,
        for<'a> E: EnvelopeVisitor<'a> + Extractor;
}

pub trait Extractor {
    type Output;
    fn get(self) -> Self::Output;
}

/// Represents an [`Extractor`] whose output is a [`Result`]
/// Useful for deriving traits that should be aware of Result Ok and Error
/// values.
pub trait TryExtractor: Extractor<Output = Result<Self::Ok, Self::Error>> {
    type Ok;
    type Error;
    /// Try to get the extraction result
    fn try_get(self) -> Result<Self::Ok, Self::Error>;
}

impl<T, O, Err> TryExtractor for T
where
    T: Extractor<Output = Result<O, Err>>,
{
    type Ok = O;

    type Error = Err;

    fn try_get(self) -> Result<Self::Ok, Self::Error> {
        self.get()
    }
}

/// Allows us to call these methods straight on the protobuf types without any
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

    fn consume<E>(&self, mut extractor: E) -> Result<E::Output, EnvelopeError>
    where
        Self: Sized,
        for<'a> E: EnvelopeVisitor<'a> + Extractor,
        for<'a> EnvelopeError: From<<E as EnvelopeVisitor<'a>>::Error>,
    {
        self.accept(&mut extractor)?;
        Ok(extractor.get())
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

    fn consume<E>(self) -> Result<Vec<<E as Extractor>::Output>, EnvelopeError>
    where
        for<'a> E: Default + Extractor + EnvelopeVisitor<'a>,
        for<'a> EnvelopeError: From<<E as EnvelopeVisitor<'a>>::Error>,
        Self: Sized,
    {
        SequencedExtractor::builder()
            .envelopes(self)
            .build::<E>()
            .get()
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
