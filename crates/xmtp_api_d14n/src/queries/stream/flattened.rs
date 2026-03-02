//! Convenience type which changes a stream whos items implement `Paged` to return the inner paged items (Vec<T>)

use futures::{Stream, TryStream};
use pin_project::pin_project;
use std::task::Poll;
use xmtp_proto::api_client::Paged;

use crate::protocol::EnvelopeError;

#[pin_project]
pub struct FlattenedStream<S> {
    #[pin]
    inner: S,
}

/// Wrap a `TryStream<T>` whos items, TryStream::<T>::Ok, implement
/// [`Paged`](xmtp_proto::api_client::Paged).
/// functionally, this means a struct wrapping a Vec<T> will return the inner Vec<T> instead of the
/// struct
///
/// in other words, a Stream of the form `Stream<Item = Foo>`
/// will instead be `Stream<Item = Vec<Boo>>`
/// ```ignore
///     struct Foo {
///         items: Vec<Boo>
///     }
///
///     impl Paged for Foo {
///         type Message = Boo;
///         fn messages(self) -> Vec<Boo> {
///             self.items
///         }
///         /* .. */
///     }
/// ```
///
pub fn flattened<S>(s: S) -> FlattenedStream<S> {
    FlattenedStream::<S> { inner: s }
}

impl<S> Stream for FlattenedStream<S>
where
    S: TryStream,
    S::Ok: Paged,
    S::Error: From<EnvelopeError>,
{
    type Item = Result<Vec<<S::Ok as Paged>::Message>, S::Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.as_mut().project();
        this.inner.try_poll_next(cx).map_ok(Paged::messages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use futures::stream;
    use xmtp_proto::xmtp::mls::api::v1::PagingInfo;

    #[derive(Debug, Clone)]
    struct TestPagedResponse {
        items: Vec<String>,
    }

    impl Paged for TestPagedResponse {
        type Message = String;

        fn info(&self) -> &Option<PagingInfo> {
            &None
        }

        fn messages(self) -> Vec<Self::Message> {
            self.items
        }
    }

    #[xmtp_common::test]
    async fn test_flattened_stream() {
        // Create a stream of TestPagedResponse items
        let test_data: Vec<Result<TestPagedResponse, EnvelopeError>> = vec![
            Ok(TestPagedResponse {
                items: vec!["message1".to_string(), "message2".to_string()],
            }),
            Ok(TestPagedResponse {
                items: vec!["message3".to_string()],
            }),
            Ok(TestPagedResponse {
                items: vec![
                    "message4".to_string(),
                    "message5".to_string(),
                    "message6".to_string(),
                ],
            }),
        ];

        let source_stream = stream::iter(test_data);
        let mut flattened = flattened(source_stream);

        // First batch
        let result1 = flattened.next().await.unwrap().unwrap();
        assert_eq!(
            result1,
            vec!["message1".to_string(), "message2".to_string()]
        );

        // Second batch
        let result2 = flattened.next().await.unwrap().unwrap();
        assert_eq!(result2, vec!["message3".to_string()]);

        // Third batch
        let result3 = flattened.next().await.unwrap().unwrap();
        assert_eq!(
            result3,
            vec![
                "message4".to_string(),
                "message5".to_string(),
                "message6".to_string()
            ]
        );

        // Stream should be exhausted
        assert!(flattened.next().await.is_none());
    }
}
