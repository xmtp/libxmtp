use std::sync::Arc;

use xmtp_api_grpc::error::GrpcError;
use xmtp_proto::{
    api::{ApiClientError, Client, IsConnectedCheck},
    api_client::ToDynApi,
};

use crate::{BoxedStreamsClient, V3Client};

impl<C> ToDynApi for V3Client<C>
where
    C: Send + Sync + Client<Error = GrpcError> + IsConnectedCheck + 'static,
    <C as Client>::Stream: 'static,
{
    type Error = ApiClientError<GrpcError>;

    fn boxed(self) -> xmtp_proto::api_client::BoxedXmtpApi<Self::Error> {
        Box::new(BoxedStreamsClient::new(self))
    }

    fn arced(self) -> xmtp_proto::api_client::ArcedXmtpApi<Self::Error> {
        Arc::new(BoxedStreamsClient::new(self))
    }
}
