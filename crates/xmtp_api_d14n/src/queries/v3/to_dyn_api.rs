use std::sync::Arc;

use crate::protocol::{FullXmtpApiArc, FullXmtpApiBox};
use crate::{ToDynApi, protocol::CursorStore};
use xmtp_common::RetryableError;
use xmtp_proto::api::{ApiClientError, Client, IsConnectedCheck};

use crate::{BoxedStreamsClient, V3Client};

impl<C, Store, E> ToDynApi for V3Client<C, Store>
where
    E: RetryableError + 'static,
    C: Client<Error = E> + IsConnectedCheck + 'static,
    <C as Client>::Stream: 'static,
    Store: CursorStore + 'static,
{
    type Error = ApiClientError<E>;
    fn boxed(self) -> FullXmtpApiBox<Self::Error> {
        Box::new(BoxedStreamsClient::new(self))
    }

    fn arced(self) -> FullXmtpApiArc<Self::Error> {
        Arc::new(BoxedStreamsClient::new(self))
    }
}
