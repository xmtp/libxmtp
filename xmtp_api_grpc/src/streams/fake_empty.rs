//! Create a fake stream that will always be pending
//! this is a workaround for https://github.com/xmtp/xmtpd/issues/1440

use prost::bytes::Bytes;
use std::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};
use xmtp_common::RetryableError;
use xmtp_proto::prelude::ApiClientError;

use futures::{Stream, stream::FusedStream};

/// This stream will always return Pending
/// it should be used when subscribing with an empty topics list
#[derive(Default, Clone)]
pub struct FakeEmptyStream<E> {
    _error: PhantomData<E>,
}

impl<E> FakeEmptyStream<E> {
    pub fn new() -> Self {
        Self {
            _error: PhantomData,
        }
    }
}

impl<E> Stream for FakeEmptyStream<E>
where
    E: RetryableError,
{
    type Item = Result<hyper::body::Frame<Bytes>, ApiClientError<E>>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Pending
    }
}

impl<T> Unpin for FakeEmptyStream<T> {}

impl<E: RetryableError> FusedStream for FakeEmptyStream<E> {
    fn is_terminated(&self) -> bool {
        true
    }
}
