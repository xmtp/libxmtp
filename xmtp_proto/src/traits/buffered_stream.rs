use crate::api::{ApiClientError, XmtpStream};
use futures::{
    SinkExt, Stream, StreamExt, TryStream,
    channel::mpsc::{self, Receiver},
};
use pin_project_lite::pin_project;
use prost::bytes::Bytes;
use std::{
    marker::PhantomData,
    pin::{Pin, pin},
    task::{Context, Poll},
};
use xmtp_common::MaybeSend;

pub trait StreamBufferExt<S, Item>
where
    S: TryStream<Ok = Bytes> + MaybeSend,
    Item: prost::Message + Default + 'static,
    <S as TryStream>::Error: std::error::Error + MaybeSend,
{
    fn buffered(self) -> XmtpBufferedStream<S, Item>;
}

impl<S, Item> StreamBufferExt<S, Item> for XmtpStream<S, Item>
where
    S: TryStream<Ok = Bytes> + MaybeSend + 'static,
    Item: prost::Message + Default + 'static,
    <S as TryStream>::Error: std::error::Error + MaybeSend + 'static,
{
    fn buffered(self) -> XmtpBufferedStream<S, Item> {
        XmtpBufferedStream::new(self)
    }
}

const BUFFER_MAX: usize = 1_000;
pin_project! {
    /// A buffer that wraps around the stream to avoid backpressure to the server
    /// which may result in potential lost messages.
    pub struct XmtpBufferedStream<S, Item>
    where
        S: TryStream<Ok = Bytes>,
        <S as TryStream>::Error: std::error::Error
    {
        #[pin] rx: Receiver<Result<Item, ApiClientError<S::Error>>>,
        _stream: PhantomData<S>,
    }
}

impl<S, Item> XmtpBufferedStream<S, Item>
where
    S: TryStream<Ok = Bytes>,
    Item: prost::Message + Default + 'static,
    S::Error: std::error::Error + MaybeSend + 'static,
{
    pub fn new(
        inner: impl Stream<Item = Result<Item, ApiClientError<S::Error>>> + MaybeSend + 'static,
    ) -> Self {
        let (mut tx, rx) = mpsc::channel(BUFFER_MAX);
        xmtp_common::spawn(None, async move {
            let mut pinned = pin!(inner);
            while let Some(next) = pinned.as_mut().next().await {
                if tx.send(next).await.is_err() {
                    break;
                }
            }
        });

        Self {
            rx,
            _stream: PhantomData,
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
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().rx.poll_next(cx)
    }
}
