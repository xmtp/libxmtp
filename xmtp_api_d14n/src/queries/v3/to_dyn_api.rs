use std::sync::Arc;

use crate::protocol::{FullXmtpApiArc, FullXmtpApiBox};
use crate::{ToDynApi, protocol::CursorStore};
use xmtp_api_grpc::error::GrpcError;
use xmtp_proto::api::{Client, IsConnectedCheck};

use crate::{BoxedStreamsClient, V3Client};

impl<C, Store> ToDynApi for V3Client<C, Store>
where
    // E: Error + RetryableError + Send + Sync + 'static,
    C: Send + Sync + Client<Error = GrpcError> + IsConnectedCheck + 'static,
    <C as Client>::Stream: 'static,
    Store: CursorStore + 'static,
{
    fn boxed(self) -> FullXmtpApiBox {
        Box::new(BoxedStreamsClient::new(self))
    }

    fn arced(self) -> FullXmtpApiArc {
        Arc::new(BoxedStreamsClient::new(self))
    }
}
