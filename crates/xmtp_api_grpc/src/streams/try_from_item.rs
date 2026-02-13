//! Maps a `TryStream` with a different type that implements `TryFrom<T>` for the streams item

use futures::{Stream, TryStream};
use pin_project::pin_project;
use std::{
    marker::PhantomData,
    task::{Poll, ready},
};

#[pin_project]
pub struct TryFromItem<S, T> {
    #[pin]
    inner: S,
    _marker: PhantomData<T>,
}

/// Wrap a `TryStream<T>` such that it converts its 'item' to T
pub fn try_from_stream<S, T>(s: S) -> TryFromItem<S, T> {
    TryFromItem::<S, T> {
        inner: s,
        _marker: PhantomData,
    }
}

impl<S, T> Stream for TryFromItem<S, T>
where
    S: TryStream,
    T: TryFrom<S::Ok>,
    S::Error: From<<T as TryFrom<S::Ok>>::Error>,
{
    type Item = Result<T, S::Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.as_mut().project();
        let item = ready!(this.inner.try_poll_next(cx));
        match item {
            Some(i) => Poll::Ready(Some(i.and_then(|i| i.try_into().map_err(S::Error::from)))),
            None => Poll::Ready(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{StreamExt, stream};
    use rstest::rstest;

    #[derive(Debug, PartialEq, Clone)]
    struct TestItem {
        value: u32,
    }

    #[derive(Debug, PartialEq)]
    enum TestError {
        ConversionError(String),
    }

    impl std::fmt::Display for TestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                TestError::ConversionError(msg) => write!(f, "Conversion error: {}", msg),
            }
        }
    }

    impl std::error::Error for TestError {}

    impl TryFrom<u32> for TestItem {
        type Error = TestError;

        fn try_from(value: u32) -> Result<Self, Self::Error> {
            if value == 999 {
                Err(TestError::ConversionError("Invalid value 999".to_string()))
            } else {
                Ok(TestItem { value })
            }
        }
    }

    #[rstest]
    #[case(vec![Ok(1), Ok(2), Ok(3)], vec![TestItem { value: 1 }, TestItem { value: 2 }, TestItem { value: 3 }], "happy_path")]
    #[case(vec![], vec![], "empty_stream")]
    #[case(vec![Ok(42)], vec![TestItem { value: 42 }], "single_item_stream")]
    #[case(vec![Ok(1), Ok(2), Ok(3), Ok(4), Ok(5)], vec![TestItem { value: 1 }, TestItem { value: 2 }, TestItem { value: 3 }, TestItem { value: 4 }, TestItem { value: 5 }], "multiple_items_stream")]
    #[xmtp_common::test]
    async fn test_successful_conversions(
        #[case] input: Vec<Result<u32, TestError>>,
        #[case] expected: Vec<TestItem>,
        #[case] _description: &str,
    ) {
        let stream = stream::iter(input);
        let try_from_stream = try_from_stream::<_, TestItem>(stream);

        let results: Vec<_> = try_from_stream.map(Result::unwrap).collect().await;
        assert_eq!(results, expected);
    }

    #[xmtp_common::test]
    async fn test_conversion_error_propagation() {
        let items: Vec<Result<u32, TestError>> = vec![Ok(1), Ok(999), Ok(3)];
        let stream = stream::iter(items);
        let try_from_stream = try_from_stream::<_, TestItem>(stream);

        let results: Vec<_> = try_from_stream.collect().await;
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].as_ref().unwrap(), &TestItem { value: 1 });
        assert!(matches!(
            results[1].as_ref().unwrap_err(),
            TestError::ConversionError(_)
        ));
        assert_eq!(results[2].as_ref().unwrap(), &TestItem { value: 3 });
    }

    #[xmtp_common::test]
    fn stream_can_finish() {
        let items: Vec<Result<u32, TestError>> = vec![Ok(42)];
        let stream = stream::iter(items);
        let stream = try_from_stream::<_, TestItem>(stream);
        futures::pin_mut!(stream);

        let mut cx = futures_test::task::noop_context();
        assert!(matches!(
            stream.as_mut().poll_next(&mut cx),
            Poll::Ready(Some(Ok(_)))
        ));

        let mut cx = futures_test::task::noop_context();
        assert!(matches!(stream.poll_next(&mut cx), Poll::Ready(None)));
    }

    #[xmtp_common::test]
    fn happy_path() {
        let items: Vec<Result<u32, TestError>> = vec![Ok(1), Ok(2)];
        let stream = stream::iter(items);
        let stream = try_from_stream::<_, TestItem>(stream);
        futures::pin_mut!(stream);

        let mut cx = futures_test::task::noop_context();

        // Poll first item
        let first_poll = stream.as_mut().poll_next(&mut cx);
        assert!(matches!(first_poll, Poll::Ready(Some(Ok(_)))));
        if let Poll::Ready(Some(Ok(item))) = first_poll {
            assert_eq!(item, TestItem { value: 1 });
        }

        // Poll second item
        let second_poll = stream.as_mut().poll_next(&mut cx);
        assert!(matches!(second_poll, Poll::Ready(Some(Ok(_)))));
        if let Poll::Ready(Some(Ok(item))) = second_poll {
            assert_eq!(item, TestItem { value: 2 });
        }

        // Poll for end
        let end_poll = stream.poll_next(&mut cx);
        assert!(matches!(end_poll, Poll::Ready(None)));
    }
}
