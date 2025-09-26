use std::{error::Error, sync::Arc};

use xmtp_common::RetryableError;
use xmtp_proto::{
    api::{ApiClientError, Client},
    api_client::ToDynApi,
};

use crate::{BoxedStreamsClient, D14nClient};

impl<M, P, E> ToDynApi for D14nClient<M, P>
where
    E: Error + RetryableError + Send + Sync + 'static,
    P: Send + Sync + Client<Error = E> + 'static,
    M: Send + Sync + Client<Error = E> + 'static,
    <M as Client>::Stream: 'static,
    <P as Client>::Stream: 'static,
{
    type Error = ApiClientError<E>;

    fn boxed(self) -> xmtp_proto::api_client::BoxedXmtpApiWithStreams<Self::Error> {
        Box::new(BoxedStreamsClient::new(self))
    }

    fn arced(self) -> xmtp_proto::api_client::ArcedXmtpApiWithStreams<Self::Error> {
        Arc::new(BoxedStreamsClient::new(self))
    }
}
