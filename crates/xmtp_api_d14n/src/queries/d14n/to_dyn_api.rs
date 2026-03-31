use std::sync::Arc;

use xmtp_proto::api::ApiClientError;
use xmtp_proto::api::{Client, IsConnectedCheck};

use crate::protocol::FullXmtpApiArc;
use crate::protocol::FullXmtpApiBox;
use crate::{BoxedStreamsClient, D14nClient, ToDynApi, protocol::CursorStore};

impl<M, Store> ToDynApi for D14nClient<M, Store>
where
    M: Client + IsConnectedCheck + 'static,
    Store: CursorStore + Clone + 'static,
{
    type Error = ApiClientError;
    fn boxed(self) -> FullXmtpApiBox<Self::Error> {
        Box::new(BoxedStreamsClient::new(self))
    }

    fn arced(self) -> FullXmtpApiArc<Self::Error> {
        Arc::new(BoxedStreamsClient::new(self))
    }
}
