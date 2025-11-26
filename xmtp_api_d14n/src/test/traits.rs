//! XmtpTestClient impl for real docker/dev backend
use crate::protocol::CursorStore;
use std::sync::Arc;
use xmtp_proto::prelude::XmtpTestClient;

/// extends the [`XmtpTestClient`] with a cursor store build method
pub trait XmtpTestClientExt: XmtpTestClient {
    fn with_cursor_store(_store: Arc<dyn CursorStore>) -> <Self as XmtpTestClient>::Builder {
        unimplemented!(
            "cursor store not available for this type {}",
            std::any::type_name::<Self>()
        )
    }
}
