use crate::api::{ApiClientError, XmtpStream};
use bytes::Bytes;
use futures::{
    SinkExt, Stream, StreamExt, TryStream,
    channel::mpsc::{self, Receiver},
};
use pin_project_lite::pin_project;
use std::{
    pin::{Pin, pin},
    task::{Context, Poll},
};
use xmtp_common::MaybeSend;

pub trait StreamBufferExt<Item> {
    fn buffered(self) -> XmtpBufferedStream<Item>;
}

impl<S, T> StreamBufferExt<Result<T, ApiClientError<<S as TryStream>::Error>>> for XmtpStream<S, T>
where
    S: TryStream<Ok = Bytes> + MaybeSend + 'static,
    <S as TryStream>::Error: std::error::Error + MaybeSend,
    T: prost::Message + Default + 'static,
{
    fn buffered(self) -> XmtpBufferedStream<Result<T, ApiClientError<<S as TryStream>::Error>>> {
        XmtpBufferedStream::new(self)
    }
}

const BUFFER_MAX: usize = 1_000;
pin_project! {
    /// A buffer that wraps around the stream to avoid backpressure to the server
    /// which may result in potential lost messages.
    pub struct XmtpBufferedStream<Item> {
        #[pin] rx: Receiver<Item>,
    }
}

impl<Item> XmtpBufferedStream<Item>
where
    Item: MaybeSend + 'static,
{
    pub fn new(inner: impl Stream<Item = Item> + MaybeSend + 'static) -> Self {
        let (mut tx, rx) = mpsc::channel(BUFFER_MAX);
        xmtp_common::spawn(None, async move {
            let mut pinned = pin!(inner);
            while let Some(next) = pinned.as_mut().next().await {
                if tx.send(next).await.is_err() {
                    break;
                }
            }
        });

        Self { rx }
    }
}

impl<T> Stream for XmtpBufferedStream<T> {
    type Item = T;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().rx.poll_next(cx)
    }
}
