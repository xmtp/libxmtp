//! Deletion items for expired messages: whatever removes the message, the
//! deletion stream reports which message went, never what it said.

use crate::context::CoreContext;
use crate::core::events::InternalEvent;
use crate::messages::decoded_message::{DecodedMessage, DeletedBy, MessageBody};
use crate::tester;
use crate::worker::disappearing_messages::{
    DisappearingMessagesWorker, test_support::delete_expired_messages,
};
use xmtp_common::time::{Duration, now_ns};
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_db::prelude::*;
use xmtp_events::{EventBus, EventFilter, EventWriter};
use xmtp_mls_common::group::GroupMetadataOptions;
use xmtp_mls_common::group_mutable_metadata::MessageDisappearingSettings;
use xmtp_proto::types::GroupId;

/// The longest a test waits for a stored expiry.
const EXPIRY_WAIT_LIMIT: Duration = Duration::from_secs(30);

/// Wait until the stored expiry of `id` has passed. Confirmation replaces
/// the local send time with the backend's, so the stored value can be
/// later than the local clock. A skewed backend clock fails the test
/// instead of hanging it.
async fn wait_until_expired(context: &impl CoreContext, id: &[u8]) {
    let expire_at = context
        .db()
        .get_group_message(id)
        .unwrap()
        .and_then(|message| message.expire_at_ns)
        .expect("an expiring message");
    let now = now_ns();
    let wait = Duration::from_nanos(expire_at.saturating_sub(now).max(0) as u64);
    assert!(
        wait <= EXPIRY_WAIT_LIMIT,
        "stored expire_at_ns {expire_at} is {wait:?} after now_ns {now}, more than \
         {EXPIRY_WAIT_LIMIT:?}; the backend clock is probably ahead of the local clock"
    );
    while now_ns() < expire_at {
        let wait = (expire_at - now_ns()).max(0) as u64;
        xmtp_common::time::sleep(Duration::from_nanos(wait) + Duration::from_millis(1)).await;
    }
}

/// A deletion item for an expired message is the deleted-message
/// placeholder, so it carries no body and no host decodes an empty one.
fn assert_no_body(
    deleted: &DecodedMessage,
    group_id: &GroupId,
    message_id: &[u8],
    sender_inbox_id: &str,
    deleted_by: &DeletedBy,
) {
    use crate::messages::enrichment::deleted_message_content_type;

    assert_eq!(deleted.metadata.group_id, *group_id);
    assert_eq!(deleted.metadata.id, message_id);
    assert_eq!(deleted.metadata.sender_inbox_id, sender_inbox_id);
    assert_eq!(
        deleted.metadata.content_type,
        Some(deleted_message_content_type()),
        "the deletion item has no content type"
    );
    match &deleted.content {
        MessageBody::DeletedMessage { deleted_by: actual } => {
            assert_eq!(
                actual, deleted_by,
                "the deletion item names the wrong actor"
            )
        }
        other => panic!("the deletion item carried a body: {other:?}"),
    }
    assert_eq!(deleted.fallback_text, None);
}

/// The deletion stream of `context`, not polled until the test reads it.
fn deletion_stream(
    context: &impl CoreContext,
) -> impl futures::Stream<Item = crate::subscriptions::Result<DecodedMessage>> {
    use crate::subscriptions::StreamMessages;
    context
        .events()
        .subscribe(
            EventFilter::default().with_internal(|event| {
                matches!(
                    event,
                    InternalEvent::MessagesDeleted(_) | InternalEvent::MessagesExpired(_)
                )
            }),
            Some(8),
        )
        .stream_message_deletions()
}

/// Encoded text with a fallback, so a test can see whether either leaks.
fn secret_text() -> Vec<u8> {
    use xmtp_content_types::{ContentCodec, encoded_content_to_bytes, text::TextCodec};
    let mut secret = TextCodec::encode("secret".into()).unwrap();
    secret.fallback = Some("secret fallback".into());
    encoded_content_to_bytes(secret)
}

/// The three ways an expired message leaves the database: expiry cleanup,
/// a group delete, and a local client delete.
#[derive(Clone, Copy, Debug)]
enum Removal {
    Cleanup,
    GroupDelete,
    ClientDelete,
}

/// Every deletion of an expired message reports which message was
/// deleted, not what it said.
// verifies: META-051
#[rstest::rstest]
#[case::cleanup(Removal::Cleanup)]
#[case::group_delete(Removal::GroupDelete)]
#[case::client_delete(Removal::ClientDelete)]
#[xmtp_common::test(unwrap_try = true)]
async fn expired_message_deletion_event_carries_no_body(
    #[case] removal: Removal,
) -> Result<(), Box<dyn std::error::Error>> {
    use futures::StreamExt;

    tester!(alix, disable_workers);
    let group = alix.create_group(
        None,
        Some(GroupMetadataOptions {
            message_disappearing_settings: Some(MessageDisappearingSettings::new(1, 1)),
            ..Default::default()
        }),
    )?;
    let deletions = deletion_stream(&alix.context);
    futures::pin_mut!(deletions);
    let message_id = group
        .send_message(&secret_text(), Default::default())
        .await?;
    wait_until_expired(&alix.context, &message_id).await;

    match removal {
        Removal::Cleanup => {
            delete_expired_messages(&mut DisappearingMessagesWorker::new(alix.context.clone()))
                .await?;
        }
        Removal::GroupDelete => {
            // The deletion applies at its echo.
            group.delete_message(message_id.clone())?;
            group.publish_messages().await?;
        }
        Removal::ClientDelete => {
            assert_eq!(alix.delete_message(message_id.clone())?, 1);
        }
    }
    let deleted = xmtp_common::time::timeout(Duration::from_secs(5), deletions.next())
        .await?
        .expect("a deletion item")?;
    assert_no_body(
        &deleted,
        &group.group_id,
        &message_id,
        alix.inbox_id(),
        &DeletedBy::Sender,
    );
    Ok(())
}

/// A deletion item can wait in the broadcast buffer until after its
/// message expired. It must not deliver the body then. The item is live
/// when emitted and its row is expired when read, with no wait.
// verifies: META-051
#[xmtp_common::test(unwrap_try = true)]
async fn deletion_that_expires_before_delivery_carries_no_body() {
    use crate::subscriptions::{StreamMessages, internal::DeletedMessages};
    use futures::StreamExt;
    use xmtp_db::group_message::{ContentType, DeliveryStatus, GroupMessageKind};

    let bus = EventBus::new();
    let deletions = bus
        .subscribe(
            EventFilter::default().with_internal(|event| {
                matches!(
                    event,
                    InternalEvent::MessagesDeleted(_) | InternalEvent::MessagesExpired(_)
                )
            }),
            Some(8),
        )
        .stream_message_deletions();
    futures::pin_mut!(deletions);
    let group_id = GroupId::from([9; 16]);
    let message_id = vec![0xE1, 0x05];
    let sender_inbox_id = "sender";
    let row = StoredGroupMessage {
        id: message_id.clone(),
        group_id,
        decrypted_message_bytes: secret_text(),
        sent_at_ns: now_ns() - 2,
        kind: GroupMessageKind::Application,
        sender_installation_id: vec![1, 2, 3],
        sender_inbox_id: sender_inbox_id.into(),
        delivery_status: DeliveryStatus::Published,
        content_type: ContentType::Text,
        version_major: 1,
        version_minor: 0,
        authority_id: "xmtp.org".into(),
        reference_id: None,
        expiry_unknown: false,
        deleted_by_inbox_id: None,
        expire_at_ns: Some(now_ns() - 1),
        sequence_id: 1,
        envelope_hash: None,
        expiry_ns: None,
        inserted_at_ns: 0,
        should_push: false,
        idempotency_key: String::new(),
    };
    bus.emit(
        None,
        Some(InternalEvent::MessagesDeleted(DeletedMessages {
            messages: vec![row],
            deleted_by_inbox_id: Some(sender_inbox_id.into()),
        })),
    );

    let deleted = xmtp_common::time::timeout(Duration::from_secs(5), deletions.next())
        .await?
        .expect("a deletion item")?;
    assert_no_body(
        &deleted,
        &group_id,
        &message_id,
        sender_inbox_id,
        &DeletedBy::Sender,
    );
}

/// Where a super admin's delete of an expired message is observed.
#[derive(Clone, Copy, Debug)]
enum Observer {
    /// The admin's own installation, which deletes locally.
    Admin,
    /// The sender's installation, which receives the delete.
    Sender,
}

/// A super admin's delete of an expired message names the admin, on the
/// deleting installation and on a receiving one.
// verifies: META-051
#[rstest::rstest]
#[case::local(Observer::Admin)]
#[case::remote(Observer::Sender)]
#[xmtp_common::test(unwrap_try = true)]
async fn admin_delete_of_expired_message_names_the_admin(
    #[case] observer: Observer,
) -> Result<(), Box<dyn std::error::Error>> {
    use futures::StreamExt;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let alix_group = alix.create_group(
        None,
        Some(GroupMetadataOptions {
            message_disappearing_settings: Some(MessageDisappearingSettings::new(1, 1)),
            ..Default::default()
        }),
    )?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.group(&alix_group.group_id)?;
    let deletions = match observer {
        Observer::Admin => deletion_stream(&alix.context).left_stream(),
        Observer::Sender => deletion_stream(&bo.context).right_stream(),
    };
    futures::pin_mut!(deletions);

    let message_id = bo_group
        .send_message(&secret_text(), Default::default())
        .await?;
    alix_group.sync().await?;
    wait_until_expired(&alix.context, &message_id).await;
    wait_until_expired(&bo.context, &message_id).await;
    alix_group.delete_message(message_id.clone())?;
    // The deletion applies at its echo, on the deleting installation too.
    alix_group.publish_messages().await?;
    if let Observer::Sender = observer {
        bo_group.sync().await?;
    }

    let deleted = xmtp_common::time::timeout(Duration::from_secs(5), deletions.next())
        .await?
        .expect("a deletion item")?;
    assert_no_body(
        &deleted,
        &alix_group.group_id,
        &message_id,
        bo.inbox_id(),
        &DeletedBy::Admin(alix.inbox_id().to_string()),
    );
    Ok(())
}
