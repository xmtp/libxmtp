//! Extracts & flattens items from a `TryStream` whose items implement [`EnvelopeCollection`] with extractor ('T')

use futures::{Stream, TryStream};
use pin_project_lite::pin_project;
use std::{
    collections::VecDeque,
    marker::PhantomData,
    task::{Poll, ready},
};

use crate::protocol::{
    EnvelopeCollection, EnvelopeError, EnvelopeVisitor, TryEnvelopeCollectionExt, TryExtractor,
};

pin_project! {
    pub struct TryExtractorStream<S, E: TryExtractor> {
        #[pin] inner: S,
        buffered: VecDeque<<E as TryExtractor>::Ok>,
        _marker: PhantomData<E>,
    }
}

/// Wrap a `TryStream<T>` such that it converts its 'item' to T
// _NOTE_: extractor accepted as argument to avoid a requirement on
// specifying `Stream` type
pub fn try_extractor<S, E>(s: S) -> TryExtractorStream<S, E>
where
    E: TryExtractor,
{
    TryExtractorStream::<S, E> {
        inner: s,
        buffered: Default::default(),
        _marker: PhantomData,
    }
}

impl<S, E> Stream for TryExtractorStream<S, E>
where
    S: TryStream,
    for<'a> S::Ok: EnvelopeCollection<'a> + std::fmt::Debug,
    for<'a> S::Error: From<EnvelopeError>,
    for<'a> E: TryExtractor + EnvelopeVisitor<'a> + Default,
    <E as TryExtractor>::Error: std::fmt::Debug,
    for<'a> EnvelopeError:
        From<<E as EnvelopeVisitor<'a>>::Error> + From<<E as TryExtractor>::Error>,
{
    type Item = Result<E::Ok, S::Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.as_mut().project();
        if let Some(item) = this.buffered.pop_front() {
            return Poll::Ready(Some(Ok(item)));
        }
        let envelope = ready!(this.inner.try_poll_next(cx));
        match envelope {
            Some(item) => {
                let item = item?;
                let (success, _failure) = item.try_consume::<E>()?;
                let mut consumed = success.into_iter();
                let ready_item = consumed.next();
                this.buffered.extend(consumed);
                if let Some(item) = ready_item {
                    return Poll::Ready(Some(Ok(item)));
                }
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            None => Poll::Ready(None),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::protocol::{EnvelopeError, EnvelopeVisitor, Extractor};
    use futures::{StreamExt, stream};
    use rstest::rstest;

    /// Stream Item type
    /// ProtocolEnvelope is implemented for u32
    /// for testing only
    type StreamItem = u32;

    // Mock extractor for testing
    #[derive(Default)]
    struct MockExtractor {
        value: StreamItem,
    }

    impl<'a> EnvelopeVisitor<'a> for MockExtractor {
        type Error = EnvelopeError;
        fn test_visit_u32(&mut self, n: &u32) -> Result<(), Self::Error> {
            self.value = *n;
            Ok(())
        }
    }

    impl Extractor for MockExtractor {
        type Output = Result<StreamItem, EnvelopeError>;

        fn get(self) -> Self::Output {
            Ok(self.value)
        }
    }

    // Error extractor for testing error cases
    #[derive(Default)]
    struct ErrorExtractor;

    impl<'a> EnvelopeVisitor<'a> for ErrorExtractor {
        type Error = EnvelopeError;
    }

    impl Extractor for ErrorExtractor {
        type Output = Result<String, EnvelopeError>;

        fn get(self) -> Self::Output {
            Err(EnvelopeError::NotFound("extractor error"))
        }
    }

    #[rstest]
    #[case(vec![Ok(vec![1]), Ok(vec![2])], vec![1, 2], "happy_path")]
    #[case(vec![], vec![], "empty_stream")]
    #[case(vec![Ok(vec![1])], vec![1], "single_item_stream")]
    #[case(vec![Ok(vec![1]), Ok(vec![2]), Ok(vec![3])], vec![1, 2, 3], "multiple_items_stream")]
    #[case(vec![Ok(vec![]), Ok(vec![1])], vec![1], "empty_collection")]
    #[case(vec![Ok(vec![1, 2]), Ok(vec![3])], vec![1, 2, 3], "buffering")]
    #[xmtp_common::test]
    async fn test_content_scenarios(
        #[case] input: Vec<Result<Vec<u32>, EnvelopeError>>,
        #[case] expected: Vec<u32>,
        #[case] _description: &str,
    ) {
        let stream = stream::iter(input);
        let extractor_stream = try_extractor::<_, MockExtractor>(stream);

        let results: Vec<_> = extractor_stream.map(Result::unwrap).collect().await;
        assert_eq!(results, expected);
    }

    #[xmtp_common::test]
    async fn test_stream_error_propagation() {
        let items: Vec<Result<Vec<u32>, EnvelopeError>> =
            vec![Ok(vec![1]), Err(EnvelopeError::NotFound("test error"))];
        let stream = stream::iter(items);
        let extractor_stream = try_extractor::<_, MockExtractor>(stream);

        let results: Vec<_> = extractor_stream.collect().await;
        assert_eq!(results.len(), 2);
        assert_eq!(*results[0].as_ref().unwrap(), 1);
        assert!(results[1].is_err());
    }

    //TODO ignored until https://github.com/xmtp/libxmtp/issues/2604
    #[ignore]
    #[xmtp_common::test]
    async fn test_extraction_error_propagation() {
        let items: Vec<Result<Vec<u32>, EnvelopeError>> = vec![Ok(vec![1])];
        let stream = stream::iter(items);
        let extractor_stream = try_extractor::<_, ErrorExtractor>(stream);

        let results: Vec<_> = extractor_stream.collect().await;
        assert_eq!(results.len(), 1);
        assert!(results[0].is_err());
    }

    #[xmtp_common::test]
    fn stream_can_finish() {
        let items: Vec<Result<Vec<u32>, EnvelopeError>> = vec![Ok(vec![1])];
        let stream = stream::iter(items);
        let stream = try_extractor::<_, MockExtractor>(stream);
        futures::pin_mut!(stream);
        let mut cx = futures_test::task::noop_context();
        assert!(matches!(
            stream.as_mut().poll_next(&mut cx),
            Poll::Ready(Some(_))
        ));

        let mut cx = futures_test::task::noop_context();
        assert!(matches!(stream.poll_next(&mut cx), Poll::Ready(None)));
    }
}
