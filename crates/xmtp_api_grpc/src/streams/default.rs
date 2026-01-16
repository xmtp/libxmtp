//! Default XMTP Streams

use prost::bytes::Bytes;
use std::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll, ready},
};

use crate::error::GrpcError;
use futures::{Stream, TryStream};
use pin_project_lite::pin_project;
use xmtp_proto::{
    ApiEndpoint,
    api::{ApiClientError, Client},
};

pin_project! {
    /// A stream which maps the tonic error to ApiClientError, and attaches endpoint metadata
    pub struct XmtpTonicStream<S, T> {
        #[pin] inner: S,
        endpoint: ApiEndpoint,
        _marker: PhantomData<T>,
    }
}

impl<S, T> XmtpTonicStream<S, T> {
    pub fn new(inner: S, endpoint: ApiEndpoint) -> Self {
        Self {
            inner,
            endpoint,
            _marker: PhantomData,
        }
    }
}

impl<T> XmtpTonicStream<crate::GrpcStream, T> {
    /// create a stream from the body of a request
    /// makes the request and starts the stream
    pub async fn from_body<B: prost::Name>(
        body: B,
        client: crate::GrpcClient,
        endpoint: ApiEndpoint,
    ) -> Result<Self, ApiClientError<GrpcError>> {
        let pnq = xmtp_proto::path_and_query::<B>();
        let request = http::Request::builder();
        let path = http::uri::PathAndQuery::try_from(pnq.as_ref())?;
        let s = client
            .stream(request, path, body.encode_to_vec().into())
            .await?;
        Ok(Self::new(s.into_body(), endpoint))
    }
}

impl<S, T> Stream for XmtpTonicStream<S, T>
where
    S: TryStream<Ok = Bytes, Error = GrpcError>,
    GrpcError: From<<S as TryStream>::Error>,
    T: prost::Message + Default,
{
    type Item = Result<T, ApiClientError<GrpcError>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.as_mut().project();
        if let Some(item) = ready!(this.inner.try_poll_next(cx)) {
            let res = item
                .map_err(|e| ApiClientError::new(self.endpoint.clone(), e))
                .and_then(|i| T::decode(i).map_err(GrpcError::from).map_err(Into::into));
            Poll::Ready(Some(res))
        } else {
            Poll::Ready(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{StreamExt, stream};
    use prost::Message;
    use rstest::rstest;

    #[derive(Clone, PartialEq, Message)]
    struct TestMessage {
        #[prost(string, tag = "1")]
        pub content: String,
    }

    impl prost::Name for TestMessage {
        const NAME: &'static str = "TestMessage";
        const PACKAGE: &'static str = "test";
        fn full_name() -> String {
            format!("{}.{}", Self::PACKAGE, Self::NAME)
        }
    }

    fn create_test_message_bytes(content: &str) -> Bytes {
        let msg = TestMessage {
            content: content.to_string(),
        };
        Bytes::from(msg.encode_to_vec())
    }

    #[rstest]
    #[case::empty_stream(vec![], vec![])]
    #[case::single_message(
        vec![Ok(create_test_message_bytes("test1"))],
        vec![TestMessage { content: "test1".to_string() }],
    )]
    #[case::multiple_messages(
        vec![
            Ok(create_test_message_bytes("msg1")),
            Ok(create_test_message_bytes("msg2")),
            Ok(create_test_message_bytes("msg3"))
        ],
        vec![
            TestMessage { content: "msg1".to_string() },
            TestMessage { content: "msg2".to_string() },
            TestMessage { content: "msg3".to_string() }
        ],
    )]
    #[xmtp_common::test]
    async fn test_successful_message_decoding(
        #[case] input: Vec<Result<Bytes, GrpcError>>,
        #[case] expected: Vec<TestMessage>,
    ) {
        let stream = stream::iter(input);
        let endpoint = ApiEndpoint::SubscribeGroupMessages;
        let stream = XmtpTonicStream::<_, TestMessage>::new(stream, endpoint);

        let results: Vec<_> = stream.map(Result::unwrap).collect().await;
        assert_eq!(results, expected);
    }

    #[xmtp_common::test]
    async fn test_error_propagation() {
        let grpc_error = GrpcError::Status(tonic::Status::unavailable("Connection failed"));
        let input = vec![
            Ok(create_test_message_bytes("msg1")),
            Err(grpc_error),
            Ok(create_test_message_bytes("msg3")),
        ];

        let stream = stream::iter(input);
        let endpoint = ApiEndpoint::QueryGroupMessages;
        let stream = XmtpTonicStream::<_, TestMessage>::new(stream, endpoint.clone());

        let results: Vec<_> = stream.collect().await;
        assert_eq!(results.len(), 3);

        assert_eq!(
            results[0].as_ref().unwrap(),
            &TestMessage {
                content: "msg1".to_string()
            }
        );

        let api_error = results[1].as_ref().unwrap_err();
        if let xmtp_proto::api::ApiClientError::ClientWithEndpoint {
            endpoint: err_endpoint,
            ..
        } = api_error
        {
            assert_eq!(*err_endpoint, endpoint.to_string());
        } else {
            panic!("Expected ClientWithEndpoint error variant");
        }

        assert_eq!(
            results[2].as_ref().unwrap(),
            &TestMessage {
                content: "msg3".to_string()
            }
        );
    }

    #[xmtp_common::test]
    fn stream_ends() {
        let input = vec![Ok(create_test_message_bytes("test"))];
        let stream = stream::iter(input);
        let endpoint = ApiEndpoint::SendGroupMessages;
        let stream = XmtpTonicStream::<_, TestMessage>::new(stream, endpoint);

        futures::pin_mut!(stream);
        let mut cx = futures_test::task::noop_context();

        let first_poll = stream.as_mut().poll_next(&mut cx);
        assert!(matches!(first_poll, Poll::Ready(Some(Ok(_)))));
        if let Poll::Ready(Some(Ok(msg))) = first_poll {
            assert_eq!(msg.content, "test");
        }

        let end_poll = stream.poll_next(&mut cx);
        assert!(matches!(end_poll, Poll::Ready(None)));
    }
}
