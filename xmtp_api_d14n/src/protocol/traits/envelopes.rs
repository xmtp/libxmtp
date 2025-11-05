//! Traits representing un-processed (extracted) and processed (extracted) protobuf types
use super::*;
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

/// Represents a Single High-Level Envelope
/// An [`Envelope`] is a [`ProtocolEnvelope`] with some [`Extractor`](super::Extractor)
/// applied to it.
/// Envelopes received from the network generally must adhere
/// to the form of envelopes expected in d14n [Node2Node Protocol](https://github.com/xmtp/XIPs/blob/main/XIPs/xip-49-decentralized-backend.md#321-originator-node).
/// In the network, Node operators are responseible for maintaining
/// a [`Cursor`](xmtp_proto::types::Cursor) per envelope.
/// Likewise, Clients form the [`ClientEnvelope`] according to the [Client Node2Node Protocol](https://github.com/xmtp/XIPs/blob/main/XIPs/xip-49-decentralized-backend.md#332-envelopes)
/// Client envelopes maintain a payload/topic with MLS and Client-specific duties.
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

// Allows us to call these methods straight on the protobuf types without any
// parsing/matching first.
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
