use std::error::Error;
use std::sync::Arc;

use xmtp_common::RetryableError;
use xmtp_proto::api::ApiClientError;
use xmtp_proto::api::{Client, IsConnectedCheck};

use crate::protocol::FullXmtpApiArc;
use crate::protocol::FullXmtpApiBox;
use crate::{BoxedStreamsClient, D14nClient, ToDynApi, protocol::CursorStore};

impl<M, G, Store, E> ToDynApi for D14nClient<M, G, Store>
where
    E: Error + RetryableError + Send + Sync + 'static,
    G: Send + Sync + Client<Error = E> + IsConnectedCheck + 'static,
    M: Send + Sync + Client<Error = E> + IsConnectedCheck + 'static,
    Store: CursorStore + 'static,
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
