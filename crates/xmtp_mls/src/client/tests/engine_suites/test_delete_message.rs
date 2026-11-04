//! The leaves of `groups/tests/test_delete_message.rs` that need the client.

use crate::groups::send_message_opts::SendMessageOpts;
use crate::tester;
use xmtp_content_types::{ContentCodec, text::TextCodec};

/// Test that stream_message_deletions receives a callback when another client deletes a message
#[xmtp_common::test(unwrap_try = true)]
async fn test_stream_message_deletions_from_other_client() {
    use crate::utils::FullXmtpClient;
    use parking_lot::Mutex;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::Notify;
    use xmtp_common::StreamHandle;

    tester!(alix);
    tester!(bo);

    // Create a group and add bo
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;

    // Bo syncs to join the group
    let bo_groups = bo.wait_for_welcomes().await?;
    assert_eq!(bo_groups.len(), 1);
    let bo_group = &bo_groups[0];

    // Alix sends a message
    let text_content = TextCodec::encode("Message to be deleted".to_string())?;
    let text_bytes = xmtp_content_types::encoded_content_to_bytes(text_content);
    let message_id = alix_group
        .send_message(&text_bytes, SendMessageOpts::default())
        .await?;

    // Bo syncs to receive the message (needed so the original message is in Bo's DB)
    bo_group.sync().await?;

    // Set up shared state for Bo's callback
    let deleted_message: Arc<Mutex<Option<crate::messages::decoded_message::DecodedMessage>>> =
        Arc::new(Mutex::new(None));
    let notify = Arc::new(Notify::new());

    let deleted_message_clone = deleted_message.clone();
    let notify_clone = notify.clone();

    // Bo sets up the deletion stream with callback
    // (Bo will receive the deletion event when syncing, since alix sent it)
    let mut handle = FullXmtpClient::stream_message_deletions_with_callback(
        Arc::new(bo.client.clone()),
        move |msg| {
            if let Ok(message) = msg {
                *deleted_message_clone.lock() = Some(message);
                notify_clone.notify_one();
            }
        },
        || {},
    );

    // Wait for stream to be ready
    handle.wait_for_ready().await;

    // Alix deletes the message and publishes
    alix_group.delete_message(message_id.clone())?;
    alix_group.publish_messages().await?;

    // Bo syncs to receive the deletion (this triggers the MessageDeleted event)
    bo_group.sync().await?;

    // Wait for callback to be called (5s timeout provides buffer for async processing)
    xmtp_common::time::timeout(Duration::from_secs(5), notify.notified())
        .await
        .expect("Timed out waiting for deletion callback");

    // Verify the stream received the correct deleted message
    let deleted_message = deleted_message
        .lock()
        .take()
        .expect("No deleted message received");
    assert_eq!(deleted_message.metadata.id, message_id);
    assert_eq!(deleted_message.metadata.sender_inbox_id, alix.inbox_id());
}
