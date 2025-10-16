//! Traits to implement functionality according to
//! <https://github.com/xmtp/XIPs/blob/main/XIPs/xip-49-decentralized-backend.md#33-client-to-node-protocol>

use crate::protocol::GroupMessageExtractor;
use crate::protocol::SequencedExtractor;
use crate::protocol::V3GroupMessageExtractor;
use crate::protocol::V3WelcomeMessageExtractor;
use crate::protocol::WelcomeMessageExtractor;
use itertools::Itertools;
use xmtp_proto::types::GlobalCursor;
use xmtp_proto::types::GroupMessage;
use xmtp_proto::types::Topic;
use xmtp_proto::types::WelcomeMessage;

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

mod cursor_store;
pub use cursor_store::*;

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

/// XMTP Query queries the network for any envelopes
/// matching the cursor criteria given.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait XmtpQuery: Send + Sync {
    type Error: RetryableError + Send + Sync + 'static;
    /// Query every [`Topic`] at [`GlobalCursor`]
    async fn query_at(
        &self,
        topic: Topic,
        at: Option<GlobalCursor>,
    ) -> Result<XmtpEnvelope, Self::Error>;
}

// hides implementation detail of XmtpEnvelope/traits
/// Envelopes from the XMTP Network received from a general [`XmtpQuery`]
pub struct XmtpEnvelope {
    inner: Box<dyn EnvelopeCollection<'static> + Send + Sync>,
}

impl XmtpEnvelope {
    pub fn new(envelope: impl EnvelopeCollection<'static> + Send + Sync + 'static) -> Self {
        Self {
            inner: Box::new(envelope) as Box<_>,
        }
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn group_messages(&self) -> Result<Vec<GroupMessage>, EnvelopeError> {
        Ok(self.inner.group_messages()?.into_iter().flatten().collect())
    }

    pub fn welcome_messages(&self) -> Result<Vec<WelcomeMessage>, EnvelopeError> {
        Ok(self
            .inner
            .welcome_messages()?
            .into_iter()
            .flatten()
            .collect())
    }
}

// TODO: Maybe we can do something with Iterators and Extractors
// to avoid the Combinators like `CollectionExtractor`?
/// A Generic Higher-Level Collection of Envelopes
pub trait EnvelopeCollection<'env> {
    /// Get the topic for an envelope
    fn topics(&self) -> Result<Vec<Topic>, EnvelopeError>;
    /// Get the payload for an envelope
    fn payloads(&self) -> Result<Vec<Payload>, EnvelopeError>;
    /// Build the ClientEnvelope
    fn client_envelopes(&self) -> Result<Vec<ClientEnvelope>, EnvelopeError>;
    /// Try to get a group message from this Envelope
    fn group_messages(&self) -> Result<Vec<Option<GroupMessage>>, EnvelopeError>;
    /// Try to get a welcome message
    fn welcome_messages(&self) -> Result<Vec<Option<WelcomeMessage>>, EnvelopeError>;
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
    #[allow(clippy::type_complexity)]
    fn try_consume<E>(
        self,
    ) -> Result<
        (
            Vec<<E as TryExtractor>::Ok>,
            Vec<<E as TryExtractor>::Error>,
        ),
        EnvelopeError,
    >
    where
        for<'a> E: TryExtractor,
        for<'a> E: Default + EnvelopeVisitor<'a>,
        for<'a> EnvelopeError: From<<E as EnvelopeVisitor<'a>>::Error>,
        EnvelopeError: From<<E as TryExtractor>::Error>,
        Self: Sized,
    {
        let (success, failure): (Vec<_>, Vec<_>) =
            self.consume::<E>()?.into_iter().partition_result();
        Ok((success, failure))
    }
}

impl<'env, T> TryEnvelopeCollectionExt<'env> for T where T: EnvelopeCollection<'env> {}

/// Represents a Single High-Level Envelope
pub trait Envelope<'env> {
    /// Extract the topic for this envelope
    fn topic(&self) -> Result<Topic, EnvelopeError>;
    /// Extract the payload for this envelope
    fn payload(&self) -> Result<Payload, EnvelopeError>;
    /// Extract the client envelope (envelope containing message payload & AAD, if any) for this
    /// envelope.
    fn client_envelope(&self) -> Result<ClientEnvelope, EnvelopeError>;
    /// Try to get a group message from this Envelope
    fn group_message(&self) -> Result<Option<GroupMessage>, EnvelopeError>;
    /// Try to get a welcome message
    fn welcome_message(&self) -> Result<Option<WelcomeMessage>, EnvelopeError>;
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
    fn topic(&self) -> Result<Topic, EnvelopeError> {
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

    fn group_message(&self) -> Result<Option<GroupMessage>, EnvelopeError> {
        let mut extractor = (
            V3GroupMessageExtractor::default(),
            GroupMessageExtractor::default(),
        );
        self.accept(&mut extractor)?;
        if let Ok(Some(v3)) = extractor.0.get() {
            return Ok(Some(v3));
        }

        match extractor.1.get() {
            Ok(v) => return Ok(Some(v)),
            Err(ExtractionError::Conversion(ConversionError::Missing { .. })) => (),
            Err(e) => return Err(e.into()),
        }

        Ok(None)
    }

    fn welcome_message(&self) -> Result<Option<WelcomeMessage>, EnvelopeError> {
        let mut extractor = (
            V3WelcomeMessageExtractor::default(),
            WelcomeMessageExtractor::default(),
        );
        self.accept(&mut extractor)?;
        match extractor.0.get() {
            Ok(v) => return Ok(Some(v)),
            Err(ConversionError::Builder(_)) | Err(ConversionError::Missing { .. }) => (),
            Err(e) => return Err(e.into()),
        }

        match extractor.1.get() {
            Ok(v) => return Ok(Some(v)),
            Err(ExtractionError::Conversion(ConversionError::Builder(_)))
            | Err(ExtractionError::Conversion(ConversionError::Missing { .. })) => (),
            Err(e) => return Err(e.into()),
        }

        Ok(None)
    }
}

impl<'env, T> EnvelopeCollection<'env> for Vec<T>
where
    T: ProtocolEnvelope<'env> + std::fmt::Debug,
{
    fn topics(&self) -> Result<Vec<Topic>, EnvelopeError> {
        self.iter()
            .map(|t| t.topic())
            .collect::<Result<Vec<Topic>, _>>()
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

    fn group_messages(&self) -> Result<Vec<Option<GroupMessage>>, EnvelopeError> {
        self.iter()
            .map(|t| t.group_message())
            .collect::<Result<Vec<Option<GroupMessage>>, EnvelopeError>>()
    }

    fn welcome_messages(&self) -> Result<Vec<Option<WelcomeMessage>>, EnvelopeError> {
        self.iter()
            .map(|t| t.welcome_message())
            .collect::<Result<Vec<Option<WelcomeMessage>>, EnvelopeError>>()
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
