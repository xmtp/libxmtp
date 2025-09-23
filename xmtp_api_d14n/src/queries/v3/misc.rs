use std::sync::Arc;

use xmtp_api_grpc::error::GrpcError;
use xmtp_proto::{
    api::{ApiClientError, Client},
    api_client::ToDynApi,
};

use crate::{BoxedStreamsClient, V3Client};

impl<C> ToDynApi for V3Client<C>
where
    C: Send + Sync + Client<Error = GrpcError> + 'static,
    <C as Client>::Stream: 'static,
{
    type Error = ApiClientError<GrpcError>;

    fn boxed(self) -> xmtp_proto::api_client::BoxedXmtpApiWithStreams<Self::Error> {
        Box::new(BoxedStreamsClient::new(self))
    }

    fn arced(self) -> xmtp_proto::api_client::ArcedXmtpApiWithStreams<Self::Error> {
        Arc::new(BoxedStreamsClient::new(self))
    }
}
