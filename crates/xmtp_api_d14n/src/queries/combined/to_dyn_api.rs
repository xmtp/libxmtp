use std::sync::Arc;

use xmtp_proto::api::ApiClientError;
use xmtp_proto::api::{Client, IsConnectedCheck};

use crate::protocol::FullXmtpApiArc;
use crate::protocol::FullXmtpApiBox;
use crate::{BoxedStreamsClient, MigrationClient, ToDynApi, protocol::CursorStore};

impl<V3, D14n, Store> ToDynApi for MigrationClient<V3, D14n, Store>
where
    V3: Client + IsConnectedCheck + 'static,
    D14n: Client + IsConnectedCheck + 'static,
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
