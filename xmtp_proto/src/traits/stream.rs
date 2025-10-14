//! Default XMTP Stream

use prost::bytes::Bytes;
use std::{
    marker::PhantomData,
    pin::{Pin, pin},
    task::{Context, Poll, ready},
};
use tokio::task::JoinHandle;

use crate::{ApiEndpoint, api::ApiClientError};
use futures::{
    SinkExt, Stream, StreamExt, TryStream,
    channel::mpsc::{self, Receiver},
};
use pin_project_lite::pin_project;

const BUFFER_MAX: usize = 1_000;

pin_project! {
    /// A stream which maps the tonic error to ApiClientError, and attaches endpoint metadata
    pub struct XmtpStream<S, T> {
        #[pin] inner: S,
        endpoint: ApiEndpoint,
        _marker: PhantomData<T>,
    }
}

pin_project! {
    /// A buffer that wraps around the stream to ensure
    pub struct XmtpBufferedStream<S, Item>
    where
        S: TryStream<Ok = Bytes>,
        <S as TryStream>::Error: std::error::Error
    {
        handle: JoinHandle<()>,
        #[pin] rx: Receiver<Result<Item, ApiClientError<S::Error>>>,
        _stream: PhantomData<S>,
    }
}

impl<S, Item> XmtpBufferedStream<S, Item>
where
    S: TryStream<Ok = Bytes>,
    Item: prost::Message + Default + 'static,
    S::Error: std::error::Error + Send + 'static,
{
    pub fn new(
        inner: impl Stream<Item = Result<Item, ApiClientError<S::Error>>> + Send + 'static,
    ) -> Self {
        let (mut tx, rx) = mpsc::channel(BUFFER_MAX);
        let handle = tokio::spawn(async move {
            let mut pinned = pin!(inner);
            while let Some(next) = pinned.as_mut().next().await {
                if let Err(_) = tx.send(next).await {
                    break;
                }
            }
        });

        Self {
            handle,
            rx,
            _stream: PhantomData,
        }
    }
}

impl<S, T> XmtpStream<S, T> {
    pub fn new(inner: S, endpoint: ApiEndpoint) -> Self {
        Self {
            inner,
            endpoint,
            _marker: PhantomData,
        }
    }
}

impl<S, Item> Stream for XmtpBufferedStream<S, Item>
where
    S: TryStream<Ok = Bytes>,
    Item: prost::Message + Default,
    S::Error: std::error::Error + 'static,
{
    type Item = Result<Item, ApiClientError<S::Error>>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.as_mut().rx.poll_next_unpin(cx)
    }
}

impl<S, T> Stream for XmtpStream<S, T>
where
    S: TryStream<Ok = Bytes>,
    T: prost::Message + Default,
    S::Error: std::error::Error + 'static,
{
    type Item = Result<T, ApiClientError<S::Error>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.as_mut().project();
        if let Some(item) = ready!(this.inner.try_poll_next(cx)) {
            let res = item
                .map_err(|e| ApiClientError::new(self.endpoint, e))
                .and_then(|i| T::decode(i).map_err(ApiClientError::<S::Error>::DecodeError));
            Poll::Ready(Some(res))
        } else {
            Poll::Ready(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{StreamExt, pin_mut, stream};
    use prost::Message;

    #[derive(prost::Message)]
    struct TestMessage {
        #[prost(string, tag = "1")]
        content: String,
    }

    #[derive(thiserror::Error, Debug)]
    enum TestError {
        #[error("mock stream error")]
        StreamError,
    }

    #[xmtp_common::test]
    async fn test_poll_next_successful_decode() {
        let test_message = TestMessage {
            content: "test content".to_string(),
        };
        let encoded_bytes = test_message.encode_to_vec();

        let inner_stream =
            stream::once(async move { Ok::<Bytes, TestError>(Bytes::from(encoded_bytes)) });
        let xmtp_stream =
            XmtpStream::<_, TestMessage>::new(inner_stream, ApiEndpoint::SubscribeGroupMessages);
        pin_mut!(xmtp_stream);

        let result = xmtp_stream.next().await.unwrap();
        assert!(result.is_ok());
        let decoded_message = result.unwrap();
        assert_eq!(decoded_message.content, "test content");
        // stream ends
        let n = xmtp_stream.next().await;
        assert!(n.is_none());
    }

    #[xmtp_common::test]
    async fn test_poll_next_error_mapping() {
        let inner_stream = stream::once(async { Err::<Bytes, TestError>(TestError::StreamError) });
        let xmtp_stream =
            XmtpStream::<_, TestMessage>::new(inner_stream, ApiEndpoint::SubscribeGroupMessages);
        pin_mut!(xmtp_stream);

        let result = xmtp_stream.next().await.unwrap();
        assert!(result.is_err());

        match result {
            Err(ApiClientError::ClientWithEndpoint { endpoint, .. }) => {
                assert_eq!(endpoint, "subscribe_group_messages");
            }
            _ => panic!("Expected ClientWithEndpoint error"),
        }
        // stream ends
        let n = xmtp_stream.next().await;
        assert!(n.is_none());
    }
}
