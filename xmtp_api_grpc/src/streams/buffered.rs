use crate::{error::GrpcError, streams::XmtpTonicStream};
use futures::TryStream;
use prost::bytes::Bytes;
use xmtp_common::MaybeSend;
use xmtp_proto::api::{ApiClientError, XmtpBufferedStream};

pub trait TonicBufferExt<Item> {
    fn buffered(self, size: usize) -> XmtpBufferedStream<Item>;
}

impl<S, T> TonicBufferExt<Result<T, ApiClientError<<S as TryStream>::Error>>>
    for XmtpTonicStream<S, T>
where
    S: TryStream<Ok = Bytes, Error = GrpcError> + MaybeSend + 'static,
    <S as TryStream>::Error: std::error::Error + MaybeSend,
    GrpcError: From<<S as TryStream>::Error>,
    T: prost::Message + Default + 'static,
{
    fn buffered(
        self,
        size: usize,
    ) -> XmtpBufferedStream<Result<T, ApiClientError<<S as TryStream>::Error>>> {
        XmtpBufferedStream::new(self, size)
    }
}
