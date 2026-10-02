use crate::{Client, XmtpError};
use std::sync::Arc;

/// @xmtp-worker
/// @xmtp-internal
/// Close a lifted Client that a cancelled constructor does not return.
#[xmtp_macro::sdk_export(native_only)]
pub async fn sdk_discard_unreturned_client(client: Arc<Client>) -> Result<(), XmtpError> {
    client.discard().await
}
