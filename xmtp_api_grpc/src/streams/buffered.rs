use crate::{error::GrpcError, streams::XmtpTonicStream};
use futures::TryStream;
use prost::bytes::Bytes;
use tonic::async_trait;
use xmtp_common::MaybeSend;
use xmtp_proto::api::{ApiClientError, XmtpBufferedStream};

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait TonicBufferExt<Item> {
    async fn buffered(self, size: usize) -> XmtpBufferedStream<Item>;
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl<S, T> TonicBufferExt<Result<T, ApiClientError<<S as TryStream>::Error>>>
    for XmtpTonicStream<S, T>
where
    S: TryStream<Ok = Bytes, Error = GrpcError> + MaybeSend + 'static,
    <S as TryStream>::Error: std::error::Error + MaybeSend,
    GrpcError: From<<S as TryStream>::Error>,
    T: prost::Message + Default + 'static,
{
    async fn buffered(
        self,
        size: usize,
    ) -> XmtpBufferedStream<Result<T, ApiClientError<<S as TryStream>::Error>>> {
        XmtpBufferedStream::new(self, size).await
    }
}
