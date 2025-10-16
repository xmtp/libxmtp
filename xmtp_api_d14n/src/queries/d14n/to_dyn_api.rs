use std::{error::Error, sync::Arc};

use xmtp_common::RetryableError;
use xmtp_proto::{
    api::{ApiClientError, Client, IsConnectedCheck},
    api_client::ToDynApi,
};

use crate::{BoxedStreamsClient, D14nClient};

impl<M, G, E> ToDynApi for D14nClient<M, G>
where
    E: Error + RetryableError + Send + Sync + 'static,
    G: Send + Sync + Client<Error = E> + IsConnectedCheck + 'static,
    M: Send + Sync + Client<Error = E> + IsConnectedCheck + 'static,
    <M as Client>::Stream: 'static,
    <G as Client>::Stream: 'static,
{
    type Error = ApiClientError<E>;

    fn boxed(self) -> xmtp_proto::api_client::BoxedXmtpApi<Self::Error> {
        Box::new(BoxedStreamsClient::new(self))
    }

    fn arced(self) -> xmtp_proto::api_client::ArcedXmtpApi<Self::Error> {
        Arc::new(BoxedStreamsClient::new(self))
    }
}
