use std::sync::Arc;

use crate::protocol::{FullXmtpApiArc, FullXmtpApiBox};
use crate::{ToDynApi, protocol::CursorStore};
use xmtp_proto::api::{ApiClientError, Client, IsConnectedCheck};

use crate::{BoxedStreamsClient, V3Client};

impl<C, Store> ToDynApi for V3Client<C, Store>
where
    C: Client + IsConnectedCheck + 'static,
    Store: CursorStore + 'static,
{
    type Error = ApiClientError;
    fn boxed(self) -> FullXmtpApiBox<Self::Error> {
        Box::new(BoxedStreamsClient::new(self))
    }

    fn arced(self) -> FullXmtpApiArc<Self::Error> {
        Arc::new(BoxedStreamsClient::new(self))
    }
}
