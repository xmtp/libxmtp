//! Aggregate extractors extract data from collections of envelopes.

use super::{EnvelopeCollection, EnvelopeError, Extractor, ProtocolEnvelope};
use crate::protocol::traits::EnvelopeVisitor;
use std::marker::PhantomData;

/// Extract Data from a collection of envelopes
/// Does not preserve sequenced data, use [`SequencedExtractor`]
/// to preserve SequenceID/OriginatorID
/// runs with one extractor.
/// Does not preserve per-envelope data, since sometimes we
/// may only care about the payload of a series of envelopes.
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
