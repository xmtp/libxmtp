//! The leaves of `groups/tests/test_delete_message.rs` that need the client.

use crate::groups::send_message_opts::SendMessageOpts;
use crate::groups::test_support::message_rows::cached_text_row;
use crate::messages::decoded_message::{DeletedBy, MessageBody};
use crate::tester;
use xmtp_content_types::{ContentCodec, text::TextCodec};
use xmtp_db::Store;
use xmtp_db::group_message::{ContentType, StoredGroupMessage};

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

/// An admin delete of a stored message that has already expired names the
/// admin and carries no body.
// verifies: META-051
#[xmtp_common::test(unwrap_try = true)]
async fn admin_delete_of_expired_message_names_the_admin() {
    use crate::context::CoreContext;
    use crate::subscriptions::{StreamMessages, internal::InternalEvent};
    use futures::StreamExt;
    use xmtp_content_types::delete_message::DeleteMessageCodec;
    use xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage;

    tester!(alix);
    let alix_group = alix.create_group(None, None)?;
    let target_id = vec![0xAD, 0x01];
    let target = StoredGroupMessage {
        expire_at_ns: Some(xmtp_common::time::now_ns() - 1),
        ..cached_text_row(alix_group.group_id, target_id.clone(), "sender", "xmtp.org")?
    };
    target.store(&alix.context.db())?;
    let delete = DeleteMessageCodec::encode(DeleteMessage {
        message_id: hex::encode(&target_id),
    })?;
    let deletion = StoredGroupMessage {
        content_type: ContentType::DeleteMessage,
        reference_id: Some(target_id.clone()),
        decrypted_message_bytes: xmtp_content_types::encoded_content_to_bytes(delete),
        ..cached_text_row(
            alix_group.group_id,
            vec![0xAD, 0x02],
            alix.inbox_id(),
            "xmtp.org",
        )?
    };
    let deletions = alix
        .context
        .events()
        .subscribe(
            xmtp_events::EventFilter::default().with_internal(InternalEvent::is_message_deletion),
            Some(8),
        )
        .stream_message_deletions();
    futures::pin_mut!(deletions);

    crate::state_tx::state_write_with_events(
        alix.context.mls_storage(),
        alix.context.events(),
        |tx, buffer| {
            let storage = tx.storage();
            alix_group.store_external_application_message(&storage, &deletion, true, buffer)?;
            Ok::<_, crate::groups::mls_sync::GroupMessageProcessingError>(
                xmtp_db::TransactionOutcome::Continue(()),
            )
        },
    )?;

    let deleted = xmtp_common::time::timeout(std::time::Duration::from_secs(5), deletions.next())
        .await?
        .expect("a deletion item")?;
    assert_eq!(deleted.metadata.id, target_id);
    assert_eq!(deleted.fallback_text, None);
    let MessageBody::DeletedMessage { deleted_by } = &deleted.content else {
        panic!("the deletion item carried a body: {:?}", deleted.content);
    };
    assert_eq!(*deleted_by, DeletedBy::Admin(alix.inbox_id().to_string()));
}
