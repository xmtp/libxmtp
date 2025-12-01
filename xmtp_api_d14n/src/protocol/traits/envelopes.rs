//! Traits representing un-processed (extracted) and processed (extracted) protobuf types
use chrono::Utc;
use xmtp_common::{MaybeSend, MaybeSync};
use xmtp_proto::types::Cursor;

use crate::protocol::{CursorExtractor, DependsOnExtractor, MlsDataExtractor, TimestampExtractor};

use super::*;
/// An low-level envelope from the network gRPC interface
/*
* WARN: ProtocolEnvelope implementation for a Vec<T>
* should be avoided, since it may cause Envelope
* to implicity act on a collection when a single envelope is expected.
* Theres a way to seal this trait implementation to
* avoid external implementations which should be done.
*/
pub trait ProtocolEnvelope<'env>: std::fmt::Debug + MaybeSend + MaybeSync {
    type Nested<'a>
    where
        Self: 'a;
    fn accept<V: EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>;
    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError>;
}

//TODO: https://github.com/xmtp/libxmtp/issues/2691
// will improve usage of timestamp/sorting/resolution, so that earlier
// networking layers do not deserialize more than necessary.
/// Represents a Single High-Level Envelope
/// An [`Envelope`] is a [`ProtocolEnvelope`] with some [`Extractor`](super::Extractor)
/// applied to it.
/// Envelopes received from the network generally must adhere
/// to the form of envelopes expected in d14n [Node2Node Protocol](https://github.com/xmtp/XIPs/blob/main/XIPs/xip-49-decentralized-backend.md#321-originator-node).
/// In the network, Node operators are responseible for maintaining
/// a [`Cursor`](xmtp_proto::types::Cursor) per envelope.
/// Likewise, Clients form the [`ClientEnvelope`] according to the [Client Node2Node Protocol](https://github.com/xmtp/XIPs/blob/main/XIPs/xip-49-decentralized-backend.md#332-envelopes)
/// Client envelopes maintain a payload/topic with MLS and Client-specific duties.
pub trait Envelope<'env>: std::fmt::Debug + MaybeSend + MaybeSync {
    /// Extract the topic for this envelope
    fn topic(&self) -> Result<Topic, EnvelopeError>;
    /// Extract the cursor for this envelope
    fn cursor(&self) -> Result<Cursor, EnvelopeError>;
    /// get the envelope this depends on.
    fn depends_on(&self) -> Result<Option<GlobalCursor>, EnvelopeError>;
    /// Extract the payload for this envelope
    fn payload(&self) -> Result<Payload, EnvelopeError>;
    /// the Mls Data bytes as a sha256 hash
    fn sha256_hash(&self) -> Result<Vec<u8>, EnvelopeError>;
    /// Get the timestamp of this envelope
    fn timestamp(&self) -> Option<chrono::DateTime<Utc>>;
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
    T: ProtocolEnvelope<'env>,
{
    fn topic(&self) -> Result<Topic, EnvelopeError> {
        let mut extractor = TopicExtractor::new();
        self.accept(&mut extractor)?;
        Ok(extractor.get()?)
    }

    fn cursor(&self) -> Result<Cursor, EnvelopeError> {
        let mut extractor = CursorExtractor::new();
        self.accept(&mut extractor)?;
        Ok(extractor.get()?)
    }

    fn depends_on(&self) -> Result<Option<GlobalCursor>, EnvelopeError> {
        let mut extractor = DependsOnExtractor::default();
        self.accept(&mut extractor)?;
        Ok(extractor.get())
    }

    fn payload(&self) -> Result<Payload, EnvelopeError> {
        let mut extractor = PayloadExtractor::new();
        self.accept(&mut extractor)?;
        Ok(extractor.get()?)
    }

    fn sha256_hash(&self) -> Result<Vec<u8>, EnvelopeError> {
        let mut extractor = MlsDataExtractor::new();
        self.accept(&mut extractor)?;
        Ok(extractor.get_sha256()?)
    }

    // TODO: Currently the only "unexpected" way for this to fail
    // would be a deserialization error, or if timestamp is
    // > 2262 A.D.
    // Deserializing/failing earlier: https://github.com/xmtp/libxmtp/issues/2691
    // would encode more invariants into these extractor types
    fn timestamp(&self) -> Option<chrono::DateTime<Utc>> {
        let mut extractor = TimestampExtractor::default();
        self.accept(&mut extractor).ok()?;
        extractor.maybe_get()
    }

    fn client_envelope(&self) -> Result<ClientEnvelope, EnvelopeError> {
        // ensures we only recurse the proto data structure once.
        let mut extractor = (
            TopicExtractor::new(),
            PayloadExtractor::new(),
            DependsOnExtractor::default(),
        );
        self.accept(&mut extractor)?;
        let topic = extractor.0.get().map_err(ExtractionError::from)?;
        let payload = extractor.1.get().map_err(ExtractionError::from)?;
        let depends_on = extractor.2.get();
        Ok(ClientEnvelope {
            aad: Some(AuthenticatedData {
                target_topic: topic.to_bytes(),
                depends_on: depends_on.map(Into::into),
            }),
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

impl<'env, T> ProtocolEnvelope<'env> for &T
where
    T: ProtocolEnvelope<'env>,
{
    type Nested<'a>
        = <T as ProtocolEnvelope<'env>>::Nested<'a>
    where
        Self: 'a;

    fn accept<V: EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        <T as ProtocolEnvelope<'env>>::accept(self, visitor)
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        <T as ProtocolEnvelope<'env>>::get_nested(self)
    }
}
