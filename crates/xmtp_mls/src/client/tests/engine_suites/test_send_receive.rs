//! The leaves of `groups/tests/test_send_receive.rs` that need the client.

use crate::context::CoreContext;
use crate::groups::PreconfiguredPolicies;
use crate::tester;
use xmtp_db::group::ConversationType;
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_db::prelude::*;
use xmtp_events::{ClientEvent, EventFilter, EventKind};
use xmtp_mls_group_state::group_metadata::GroupMetadata;

// verifies: EVENT-018
#[xmtp_common::test(unwrap_try = true)]
async fn stitched_dm_received_event_names_the_stored_group() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let bo_dm = bo.find_or_create_dm(alix.inbox_id(), None).await?;
    let alix_dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    assert_ne!(bo_dm.group_id, alix_dm.group_id);
    alix.sync_all_welcomes_and_groups(None).await?;
    bo.sync_all_welcomes_and_groups(None).await?;
    let presentation = alix.stitched_group(&bo_dm.group_id)?;
    let physical = if presentation.group_id == bo_dm.group_id {
        alix_dm.group_id
    } else {
        bo_dm.group_id
    };
    let sender = bo.group(&physical)?;
    let receiver = alix.group(&physical)?;
    let events = alix
        .context
        .events()
        .subscribe(EventFilter::new([EventKind::MessageReceived]), Some(16));
    let message_id = sender
        .send_message(b"stitched message", Default::default())
        .await?;
    receiver.receive().await?;
    let stored = alix.context.db().get_group_message(&message_id)?.unwrap();
    assert_eq!(stored.group_id, physical);
    assert_ne!(stored.group_id, presentation.group_id);
    assert!(
        presentation
            .find_messages(&MsgQueryArgs::default())?
            .iter()
            .any(|message| message.id == message_id)
    );
    assert!(
        matches!(events.drain().as_slice(), [xmtp_events::EventEnvelope { client: Some(ClientEvent::MessageReceived(message)), .. }] if message.message_id == stored.id && message.group_id == stored.group_id.as_slice())
    );
    receiver.receive().await?;
    assert!(events.drain().is_empty());
    let another_id = sender
        .send_message(b"stored through presentation", Default::default())
        .await?;
    let another = bo.context.db().get_group_message(&another_id)?.unwrap();
    crate::state_tx::state_write_with_events(
        alix.context.mls_storage(),
        alix.context.events(),
        |tx, buffer| {
            presentation.store_external_application_message(
                &tx.storage(),
                &another,
                false,
                buffer,
            )?;
            Ok::<_, crate::groups::mls_sync::GroupMessageProcessingError>(
                xmtp_db::TransactionOutcome::Continue(()),
            )
        },
    )?;
    let stored = alix.context.db().get_group_message(&another_id)?.unwrap();
    assert_ne!(stored.group_id, presentation.group_id);
    assert!(
        matches!(events.drain().as_slice(), [xmtp_events::EventEnvelope { client: Some(ClientEvent::MessageReceived(message)), .. }] if message.message_id == stored.id && message.group_id == stored.group_id.as_slice())
    );
}

#[xmtp_common::test]
async fn test_can_read_group_creator_inbox_id() {
    tester!(amal);
    let policy_set = Some(PreconfiguredPolicies::Default.to_policy_set());
    let amal_group = amal.create_group(policy_set, None).unwrap();
    amal_group.sync().await.unwrap();

    let mutable_metadata = amal_group.mutable_metadata().unwrap();
    assert_eq!(mutable_metadata.super_admin_list.len(), 1);
    assert_eq!(mutable_metadata.super_admin_list[0], amal.inbox_id());

    let protected_metadata: GroupMetadata = amal_group.metadata().await.unwrap();
    assert_eq!(
        protected_metadata.conversation_type,
        ConversationType::Group
    );

    assert_eq!(protected_metadata.creator_inbox_id, amal.inbox_id());
}
