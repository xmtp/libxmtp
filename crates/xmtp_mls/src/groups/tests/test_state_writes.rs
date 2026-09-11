use crate::context::XmtpSharedContext;
use crate::groups::{MlsGroup, group_permissions::PolicySet, send_message_opts::SendMessageOpts};
use crate::tester;
use crate::utils::test::MlsGroupExt;
use xmtp_db::{
    ConnectionExt,
    diesel::RunQueryDsl,
    group::{ConversationType, GroupMembershipState},
    prelude::QueryGroupMessage,
};
use xmtp_mls_common::group::GroupMetadataOptions;

#[xmtp_common::test(unwrap_try = true)]
async fn archive_stub_keeps_a_group_joined_by_another_writer() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix.create_group(None, None)?;
    group.invite(&bo).await?;
    let bo_group = bo.sync_welcomes().await?.pop()?;
    let before = group.epoch_authenticator().await?;

    MlsGroup::insert(
        &alix.context,
        Some(group.group_id.as_slice()),
        GroupMembershipState::Restored,
        ConversationType::Group,
        PolicySet::default(),
        GroupMetadataOptions::default(),
        None,
    )?;

    assert_eq!(group.epoch_authenticator().await?, before);
    group.test_can_talk_with(&bo_group).await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn failed_intent_insert_rolls_back_optimistic_message() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix.create_group(None, None)?;
    group.invite(&bo).await?;
    let bo_group = bo.sync_welcomes().await?.pop()?;
    let db = alix.context.db();
    db.raw_query(|conn| {
        diesel::sql_query(
            "CREATE TRIGGER fail_optimistic_intent BEFORE INSERT ON group_intents \
         BEGIN SELECT RAISE(ABORT, 'injected intent failure'); END",
        )
        .execute(conn)
    })?;
    let options = SendMessageOpts {
        idempotency_key: Some("atomic optimistic send".into()),
        ..Default::default()
    };
    assert!(
        group
            .send_message_optimistic(b"retry after rollback", options.clone())
            .is_err()
    );
    let messages = db.get_group_messages(&group.group_id, &Default::default())?;
    assert!(
        !messages
            .iter()
            .any(|message| message.decrypted_message_bytes == b"retry after rollback")
    );

    db.raw_query(|conn| diesel::sql_query("DROP TRIGGER fail_optimistic_intent").execute(conn))?;
    group.send_message_optimistic(b"retry after rollback", options)?;
    group.publish_messages().await?;
    bo_group.sync().await?;
    assert_eq!(
        bo_group.test_last_message_bytes().await?,
        Some(b"retry after rollback".to_vec())
    );
}

/// Welcome bytes, receipt progress, and rotation work commit or roll back together.
#[xmtp_common::test(unwrap_try = true)]
async fn welcome_admission_queues_rotation_atomically_before_decode() {
    use crate::mls_store::MlsStore;
    use xmtp_db::{
        incoming_envelope::{NetworkEntityKind, StreamTopic},
        prelude::{QueryIdentity, QueryIncomingEnvelope, QueryTasks},
    };
    use xmtp_proto::{
        backend_v1 as wire,
        types::{Cursor, OrderedEnvelopeBatch, Topic},
    };

    tester!(bo, disable_workers);
    let db = bo.context.db();
    let before = db.next_key_package_rotation_ns()?;
    let topic = Topic::new_welcome_message(bo.context.installation_id());
    let key = StreamTopic {
        entity_id: bo.context.installation_id().to_vec(),
        kind: NetworkEntityKind::Welcome,
    };
    let batch = OrderedEnvelopeBatch {
        topic: topic.clone(),
        after: Cursor(0),
        envelopes: [
            None,
            Some(wire::client_envelope::Payload::WelcomeMessage(
                wire::WelcomeMessage { version: None },
            )),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, payload)| wire::ServerEnvelope {
            meta: Some(wire::EnvelopeMeta {
                cursor: Some(wire::Cursor {
                    sequence_id: (index as u64 + 1) * 10,
                }),
                server_ns: 123,
                topic: Some(wire::Topic {
                    topic: topic.cloned_vec(),
                }),
                message_hash: Some(wire::MessageHash {
                    hash: Some(wire::message_hash::Hash::Sha256(vec![7; 32])),
                }),
                ..Default::default()
            }),
            envelope: Some(wire::ClientEnvelope { payload }),
        })
        .collect(),
    };
    let fail_rotation = || {
        db.raw_query(|conn| {
            diesel::sql_query(
                "CREATE TRIGGER fail_welcome_rotation BEFORE INSERT ON tasks \
             BEGIN SELECT RAISE(ABORT, 'injected rotation failure'); END",
            )
            .execute(conn)
        })
    };
    fail_rotation()?;
    let store = MlsStore::new(bo.context.clone());
    let limits = bo
        .context
        .incoming_runtime()
        .policy()
        .incoming_limits(NetworkEntityKind::Welcome);
    assert!(store.admit_incoming_batch(&batch, limits).is_err());
    assert_eq!(db.topic_progress(&key)?.received, Cursor(0));
    assert!(db.pending_envelope(&key, Cursor(10))?.is_none());
    assert_eq!(db.next_key_package_rotation_ns()?, before);
    assert!(db.get_tasks()?.is_empty());

    db.raw_query(|conn| diesel::sql_query("DROP TRIGGER fail_welcome_rotation").execute(conn))?;
    let admitted = store.admit_incoming_batch(&batch, limits)?;
    assert_eq!(admitted.inserted, 2);
    assert_eq!(db.topic_progress(&key)?.received, Cursor(20));
    assert!(db.pending_envelope(&key, Cursor(10))?.is_some());
    assert!(db.pending_envelope(&key, Cursor(20))?.is_some());
    assert!(db.next_key_package_rotation_ns()?.unwrap() < before.unwrap());
    assert_eq!(db.get_tasks()?.len(), 2);

    // Duplicate receipt does not enqueue rotation work again.
    fail_rotation()?;
    assert_eq!(store.admit_incoming_batch(&batch, limits)?.inserted, 0);
    db.raw_query(|conn| diesel::sql_query("DROP TRIGGER fail_welcome_rotation").execute(conn))?;
}

/// A removal must abandon this installation's unaccepted outgoing work in the
/// same transaction. A `Published` state change that outlives the removal can
/// never be confirmed — a re-add installs fresh state past its own echo — and
/// the publish loop prefers such an intent over every later one, so leaving it
/// behind stops the conversation from ever publishing again.
#[xmtp_common::test(unwrap_try = true)]
async fn removal_supersedes_pending_intents_and_a_readd_can_publish() {
    use xmtp_db::group_intent::{IntentKind, IntentState};
    use xmtp_db::prelude::QueryGroupIntent;

    tester!(alix);
    tester!(bo);

    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.group(&alix_group.group_id)?;
    bo_group.sync().await?;

    // Bo queues a real state change but never publishes it, then is removed.
    // The queued intent is exactly the work that must not outlive membership.
    crate::groups::intents::QueueIntent::key_update().queue(&bo_group)?;
    let queued = bo.context.db().find_group_intents(
        bo_group.group_id,
        Some(vec![IntentState::ToPublish, IntentState::Published]),
        Some(IntentKind::all().collect()),
    )?;
    assert!(!queued.is_empty(), "the test needs an unaccepted intent");

    alix_group.remove_members(&[bo.inbox_id()]).await?;
    // Receive the removal without publishing: a removed member's queued work
    // cannot reach the network anyway.
    bo_group.sync().await.ok();
    assert!(!bo_group.is_active()?);

    // Nothing unaccepted survives the removal, and no prepared bytes remain to
    // be reused against a new membership generation.
    let remaining = bo.context.db().find_group_intents(
        bo_group.group_id,
        Some(vec![IntentState::ToPublish, IntentState::Published]),
        Some(IntentKind::all().collect()),
    )?;
    assert!(
        remaining.is_empty(),
        "removal must abandon unaccepted intents, found {remaining:?}"
    );

    // The re-added installation can publish again.
    alix_group.add_members(&[bo.inbox_id()]).await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.group(&alix_group.group_id)?;
    bo_group.sync().await?;
    assert!(bo_group.is_active()?);
    bo_group
        .send_message(b"after readd", SendMessageOpts::default())
        .await?;
}

/// A snapshot-built client must still be able to store and deliver messages.
/// The delivery-sequence allocator shares `refresh_state` with network
/// progress, so a reset that clears the whole table leaves the client unable
/// to allocate a delivery number and silently disables every test built on it.
#[xmtp_common::test(unwrap_try = true)]
async fn a_snapshot_tester_can_still_allocate_delivery_sequences() {
    use std::sync::Arc;
    use xmtp_db::delivery::QueryDelivery;

    tester!(alix);
    let snapshot = Arc::new(alix.db_snapshot());
    tester!(alix2, snapshot: snapshot);

    let group = alix2.create_group(None, None)?;
    group
        .send_message(b"after snapshot", SendMessageOpts::default())
        .await?;

    let messages = group.find_messages(&Default::default())?;
    assert!(
        !messages.is_empty(),
        "a snapshot client must store messages"
    );
    // Storing a deliverable message allocates from the shared allocator row,
    // so a usable cursor proves the row survived the reset.
    let cursor = alix2.context.db().current_delivery_cursor()?;
    assert!(cursor.delivery_sequence > 0);
}
