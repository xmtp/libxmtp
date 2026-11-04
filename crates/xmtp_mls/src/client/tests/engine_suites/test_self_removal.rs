//! The leaves of `groups/tests/test_self_removal.rs` that need the client.

use crate::context::CoreContext;
use crate::groups::UpdateAdminListType;
use crate::tester;
use prost::Message;
use xmtp_content_types::{ContentCodec, group_updated::GroupUpdatedCodec};
use xmtp_db::group::GroupMembershipState;
use xmtp_db::group::GroupQueryArgs;
use xmtp_db::group_message::{GroupMessageKind, MsgQueryArgs};
use xmtp_db::pending_remove::QueryPendingRemove;
use xmtp_db::prelude::*;
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

#[xmtp_common::test(unwrap_try = true)]
async fn creator_readd_preserves_welcome_stream_cursor() {
    use xmtp_db::prelude::QueryGroup;
    tester!(creator);
    tester!(admin);
    let group = creator
        .create_group_with_members(&[admin.inbox_id().to_string()], None, None)
        .await?;
    admin.sync_welcomes().await?;
    let admin_group = admin.find_groups(GroupQueryArgs::default())?.remove(0);
    group
        .update_admin_list(UpdateAdminListType::AddSuper, admin.inbox_id().to_string())
        .await?;
    group
        .update_admin_list(
            UpdateAdminListType::RemoveSuper,
            creator.inbox_id().to_string(),
        )
        .await?;
    group.sync().await?;
    admin_group.sync().await?;
    group.leave_group().await?;
    assert_eq!(
        group.membership_state()?,
        GroupMembershipState::PendingRemove
    );
    group.sync().await?;
    let mut removed = false;
    for _ in 0..20 {
        xmtp_common::time::sleep(std::time::Duration::from_millis(250)).await;
        admin_group.sync().await.ok();
        group.sync().await.ok();
        if !group.is_active()? {
            removed = true;
            break;
        }
    }
    assert!(removed, "creator must leave before the re-add");
    admin_group.add_members(&[creator.inbox_id()]).await?;
    admin_group.sync().await?;
    creator.sync_welcomes().await?;
    let cursors = creator.db().group_cursors()?;
    assert_eq!(cursors.len(), 1);
    let stream = creator.stream_conversations(None, false).await?;
    drop(stream);
}

#[xmtp_common::test(flavor = "current_thread")]
async fn test_self_removal() {
    tester!(amal);
    tester!(bola_i1);
    tester!(bola_i2, from: bola_i1);

    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola_i1.inbox_id()]).await.unwrap();

    amal_group.sync().await.unwrap();
    bola_i1.sync_welcomes().await.unwrap();

    assert_eq!(amal_group.members().await.unwrap().len(), 2);
    // Verify the pending-remove list is empty on Amal's group
    let amal_group_pending_leave_users = amal
        .db()
        .get_pending_remove_users(&amal_group.group_id)
        .unwrap();
    assert!(amal_group_pending_leave_users.is_empty());

    let bola_i1_groups = bola_i1.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_i1_groups.len(), 1);
    let bola_i1_group = bola_i1_groups.first().unwrap();
    assert_eq!(bola_i1_group.members().await.unwrap().len(), 2);

    bola_i1_group.sync().await.unwrap();

    // Verify the pending-remove list is empty on Bola_i1's group
    let bola_i1_group_pending_leave_users = bola_i1
        .db()
        .get_pending_remove_users(&bola_i1_group.group_id)
        .unwrap();
    assert!(bola_i1_group_pending_leave_users.is_empty());

    // Verify Amal's as the super admin/admin can't leave the group and their inboxId is not added to the pendingRemoveList
    amal_group
        .leave_group()
        .await
        .expect_err("Amal should not be able to leave the group");
    let amal_group_pending_leave_users = amal
        .db()
        .get_pending_remove_users(&amal_group.group_id)
        .unwrap();
    assert!(amal_group_pending_leave_users.is_empty());

    // Amal's inboxId shouldn't be in the pending-remove list
    assert!(!amal_group_pending_leave_users.contains(&amal.inbox_id().to_string()));

    // Bola_i1 should be able to leave the group
    bola_i1_group.sync().await.unwrap();
    bola_i1_group.leave_group().await.unwrap();
    let bola_i1_group_pending_leave_users = bola_i1
        .db()
        .get_pending_remove_users(&bola_i1_group.group_id)
        .unwrap();
    tracing::info!(
        "Bola_i1_group_pending_leave_users: {:?}",
        bola_i1_group_pending_leave_users
    );
    // Bola's inboxId should be in the pending-remove list on Bola's group
    assert!(bola_i1_group_pending_leave_users.contains(&bola_i1.inbox_id().to_string()));
    assert_eq!(bola_i1_group_pending_leave_users.len(), 1);

    // Bola's state for the group should be set to PendingRemove
    let bola_i1_group_from_db = bola_i1.db().find_group(&bola_i1_group.group_id).unwrap();
    assert_eq!(
        bola_i1_group_from_db.unwrap().membership_state,
        GroupMembershipState::PendingRemove
    );

    // Amal's state for the group should not change
    amal_group.sync().await.unwrap();
    let amal_group_member_state = amal
        .db()
        .find_group(&amal_group.group_id)
        .unwrap()
        .unwrap()
        .membership_state;
    assert_eq!(amal_group_member_state, GroupMembershipState::Allowed);

    // Check Bola's other installation. Note: with the event-driven self-remove,
    // the super-admin (Amal) removes Bola promptly via the TaskRunner rather than
    // after a fixed poll interval, so Bola_i2 may observe either the transient
    // PendingRemove state or the already-removed state depending on timing. We
    // therefore assert the eventual, deterministic outcome (Bola removed) rather
    // than the transient pending window.
    bola_i2.sync_welcomes().await.unwrap();
    let bola_i2_groups = bola_i2.find_groups(GroupQueryArgs::default()).unwrap();
    assert_eq!(bola_i2_groups.len(), 1);
    let bola_i2_group = bola_i2_groups.first().unwrap();

    xmtp_common::time::sleep(std::time::Duration::from_secs(2)).await;

    let _ = bola_i1_group.sync().await;
    let _ = bola_i2_group.sync().await;
    assert!(!bola_i1_group.is_active().unwrap());
    assert!(!bola_i2_group.is_active().unwrap());
    let _ = amal_group.sync().await;
    assert_eq!(amal_group.members().await.unwrap().len(), 1);
}

// verifies: EVENT-006, GMOD-031
#[xmtp_common::test(flavor = "current_thread")]
async fn test_self_removal_simple() {
    tester!(amal);
    tester!(bola);
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();
    assert_eq!(bola_group.members().await.unwrap().len(), 2);

    // Verify Bola's membership state is Pending when first invited
    assert_eq!(
        bola_group.membership_state().unwrap(),
        GroupMembershipState::Pending
    );

    let removed = bola.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::ConversationRemoved]),
        Some(10),
    );
    bola_group.leave_group().await.unwrap();

    // Verify Bola's membership state is PendingRemove after requesting to leave
    assert_eq!(
        bola_group.membership_state().unwrap(),
        GroupMembershipState::PendingRemove
    );

    amal_group.sync().await.unwrap();
    xmtp_common::time::sleep(std::time::Duration::from_secs(2)).await;
    bola_group.sync().await.unwrap();
    xmtp_common::time::sleep(std::time::Duration::from_secs(2)).await;
    assert!(!bola_group.is_active().unwrap());
    assert!(matches!(
        removed.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::ConversationRemoved(event)), ..
        }] if event.group_id == bola_group.group_id.as_slice()
            && event.cause == xmtp_events::RemovalCause::Left
    ));
    assert_eq!(amal_group.members().await.unwrap().len(), 1);

    // Verify Amal's membership state remains Allowed
    assert_eq!(
        amal_group.membership_state().unwrap(),
        GroupMembershipState::Allowed
    );
}

// verifies: GMOD-034
#[xmtp_common::test(flavor = "current_thread")]
async fn test_self_removal_group_update_message() {
    tester!(amal);
    tester!(bola);
    let amal_group = amal.create_group(None, None).unwrap();
    amal_group.add_members(&[bola.inbox_id()]).await.unwrap();

    bola.sync_welcomes().await.unwrap();
    let bola_groups = bola.find_groups(GroupQueryArgs::default()).unwrap();
    let bola_group = bola_groups.first().unwrap();
    assert_eq!(bola_group.members().await.unwrap().len(), 2);

    bola_group.leave_group().await.unwrap();
    amal_group.sync().await.unwrap();
    xmtp_common::time::sleep(std::time::Duration::from_secs(2)).await;
    bola_group.sync().await.unwrap();
    xmtp_common::time::sleep(std::time::Duration::from_secs(2)).await;
    assert!(!bola_group.is_active().unwrap());
    assert_eq!(amal_group.members().await.unwrap().len(), 1);
    amal_group.sync().await.unwrap();
    let messages = amal_group.find_messages(&MsgQueryArgs::default()).unwrap();
    tracing::info!("{:?}", messages.len());
    let message = messages[2].clone();
    assert_eq!(message.kind, GroupMessageKind::MembershipChange);
    let encoded_content =
        EncodedContent::decode(message.decrypted_message_bytes.as_slice()).unwrap();
    let group_update = GroupUpdatedCodec::decode(encoded_content).unwrap();
    assert_eq!(group_update.added_inboxes.len(), 0);
    assert_eq!(group_update.removed_inboxes.len(), 0);
    assert_eq!(group_update.left_inboxes.len(), 1);
    assert_eq!(
        group_update.left_inboxes.first().unwrap().inbox_id,
        bola.inbox_id().to_string()
    );
}
