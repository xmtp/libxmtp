use super::{EnvelopeCollection, EnvelopeError, Extractor, ProtocolEnvelope};
use crate::protocol::traits::EnvelopeVisitor;
use std::marker::PhantomData;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::extractors::key_packages::KeyPackagesExtractor;
    use crate::protocol::extractors::test_utils::*;
    use crate::protocol::extractors::topics::TopicExtractor;
    use xmtp_proto::xmtp::xmtpv4::envelopes::OriginatorEnvelope;

    fn create_test_key_package() -> Vec<u8> {
        // Create a simple mock key package for testing
        xmtp_common::rand_vec::<32>()
    }

    #[xmtp_common::test]
    fn test_collection_extractor_single_envelope() {
        let kp_data = create_test_key_package();
        let envelope = TestEnvelopeBuilder::new()
            .with_key_package_custom(kp_data.clone())
            .build();
        let envelopes = vec![envelope];

        let extractor = CollectionExtractor::new(envelopes, KeyPackagesExtractor::new());
        let result = extractor.get().unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].key_package_tls_serialized, kp_data);
    }

    #[xmtp_common::test]
    fn test_collection_extractor_multiple_envelopes() {
        let kp_data1 = create_test_key_package();
        let kp_data2 = create_test_key_package();
        let kp_data3 = create_test_key_package();

        let envelopes = vec![
            TestEnvelopeBuilder::new()
                .with_key_package_custom(kp_data1.clone())
                .build(),
            TestEnvelopeBuilder::new()
                .with_key_package_custom(kp_data2.clone())
                .build(),
            TestEnvelopeBuilder::new()
                .with_key_package_custom(kp_data3.clone())
                .build(),
        ];

        let extractor = CollectionExtractor::new(envelopes, KeyPackagesExtractor::new());
        let result = extractor.get().unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].key_package_tls_serialized, kp_data1);
        assert_eq!(result[1].key_package_tls_serialized, kp_data2);
        assert_eq!(result[2].key_package_tls_serialized, kp_data3);
    }

    #[xmtp_common::test]
    fn test_collection_extractor_empty() {
        let envelopes: Vec<OriginatorEnvelope> = vec![];
        let extractor = CollectionExtractor::new(envelopes, KeyPackagesExtractor::new());
        let result = extractor.get().unwrap();

        assert_eq!(result.len(), 0);
    }

    #[xmtp_common::test]
    fn test_sequenced_extractor_single_envelope() {
        let kp_data = create_test_key_package();
        let envelope = TestEnvelopeBuilder::new()
            .with_key_package_custom(kp_data.clone())
            .build();
        let envelopes = vec![envelope];

        let extractor = SequencedExtractor::builder()
            .envelopes(envelopes)
            .build::<KeyPackagesExtractor>();

        let result = extractor.get().unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 1);
        assert_eq!(result[0][0].key_package_tls_serialized, kp_data);
    }

    #[xmtp_common::test]
    fn test_sequenced_extractor_multiple_envelopes() {
        let kp_data1 = create_test_key_package();
        let kp_data2 = create_test_key_package();

        let envelopes = vec![
            TestEnvelopeBuilder::new()
                .with_key_package_custom(kp_data1.clone())
                .build(),
            TestEnvelopeBuilder::new()
                .with_key_package_custom(kp_data2.clone())
                .build(),
        ];

        let extractor = SequencedExtractor::builder()
            .envelopes(envelopes)
            .build::<KeyPackagesExtractor>();

        let result = extractor.get().unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].len(), 1);
        assert_eq!(result[1].len(), 1);
        assert_eq!(result[0][0].key_package_tls_serialized, kp_data1);
        assert_eq!(result[1][0].key_package_tls_serialized, kp_data2);
    }

    #[xmtp_common::test]
    fn test_sequenced_extractor_with_topic_extractor() {
        let kp_data1 = create_test_key_package();
        let kp_data2 = create_test_key_package();

        let envelopes = vec![
            TestEnvelopeBuilder::new()
                .with_key_package_custom(kp_data1)
                .build(),
            TestEnvelopeBuilder::new()
                .with_key_package_custom(kp_data2)
                .build(),
        ];

        let extractor = SequencedExtractor::builder()
            .envelopes(envelopes)
            .build::<TopicExtractor>();

        // The SequencedExtractor will fail early when processing the first envelope
        // because mock key package data can't be deserialized
        let result = extractor.get();
        assert!(result.is_err());
    }

    #[xmtp_common::test]
    fn test_sequenced_extractor_empty() {
        let envelopes: Vec<OriginatorEnvelope> = vec![];

        let extractor = SequencedExtractor::builder()
            .envelopes(envelopes)
            .build::<KeyPackagesExtractor>();

        let result = extractor.get().unwrap();
        assert_eq!(result.len(), 0);
    }
}
