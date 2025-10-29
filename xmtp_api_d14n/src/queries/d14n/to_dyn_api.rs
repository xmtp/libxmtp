use std::sync::Arc;

use xmtp_api_grpc::error::GrpcError;
use xmtp_proto::api::{Client, IsConnectedCheck};

use crate::protocol::FullXmtpApiArc;
use crate::protocol::FullXmtpApiBox;
use crate::{BoxedStreamsClient, D14nClient, ToDynApi, protocol::CursorStore};

impl<M, G, Store> ToDynApi for D14nClient<M, G, Store>
where
    G: Send + Sync + Client<Error = GrpcError> + IsConnectedCheck + 'static,
    M: Send + Sync + Client<Error = GrpcError> + IsConnectedCheck + 'static,
    Store: CursorStore + 'static,
    <M as Client>::Stream: 'static,
    <G as Client>::Stream: 'static,
{
    fn boxed(self) -> FullXmtpApiBox {
        Box::new(BoxedStreamsClient::new(self))
    }

    fn arced(self) -> FullXmtpApiArc {
        Arc::new(BoxedStreamsClient::new(self))
    }
}
