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

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait StreamBufferExt<Item> {
    async fn buffered(self, size: usize) -> XmtpBufferedStream<Item>;
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<S, T> StreamBufferExt<Result<T, ApiClientError<<S as TryStream>::Error>>> for XmtpStream<S, T>
where
    S: TryStream<Ok = Bytes> + MaybeSend + 'static,
    <S as TryStream>::Error: std::error::Error + MaybeSend,
    T: prost::Message + Default + 'static,
{
    async fn buffered(
        self,
        size: usize,
    ) -> XmtpBufferedStream<Result<T, ApiClientError<<S as TryStream>::Error>>> {
        XmtpBufferedStream::new(self, size).await
    }
}

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
    pub async fn new(inner: impl Stream<Item = Item> + MaybeSend + 'static, size: usize) -> Self {
        let (mut tx, rx) = mpsc::channel(size);
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
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.as_mut().project();
        this.rx.poll_next(cx)
    }
}
