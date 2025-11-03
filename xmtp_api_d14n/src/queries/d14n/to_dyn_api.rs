use std::{error::Error, sync::Arc};

use xmtp_common::RetryableError;
use xmtp_proto::api::{ApiClientError, Client, IsConnectedCheck};

use crate::{BoxedStreamsClient, D14nClient, FullXmtpApiArc, FullXmtpApiBox, ToDynApi};

impl<M, G, E> ToDynApi for D14nClient<M, G>
where
    E: Error + RetryableError + 'static,
    G: Client<Error = E> + IsConnectedCheck + 'static,
    M: Client<Error = E> + IsConnectedCheck + 'static,
    <M as Client>::Stream: 'static,
    <G as Client>::Stream: 'static,
{
    type Error = ApiClientError<E>;

    fn boxed(self) -> FullXmtpApiBox<Self::Error> {
        Box::new(BoxedStreamsClient::new(self))
    }

    fn arced(self) -> FullXmtpApiArc<Self::Error> {
        Arc::new(BoxedStreamsClient::new(self))
    }
}
