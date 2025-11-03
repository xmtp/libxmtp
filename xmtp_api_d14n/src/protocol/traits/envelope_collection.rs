//! Traits and blanket implementations representing a collection of [`Envelope`]'s
use xmtp_common::{MaybeSend, MaybeSync};

use super::*;

// TODO: Maybe we can do something with Iterators and Extractors
// to avoid the Combinators like `CollectionExtractor`?
/// A Generic Higher-Level Collection of Envelopes
pub trait EnvelopeCollection<'env>: MaybeSend + MaybeSync {
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

impl<'env, T> EnvelopeCollection<'env> for Vec<T>
where
    T: ProtocolEnvelope<'env> + std::fmt::Debug + MaybeSend + MaybeSync,
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
