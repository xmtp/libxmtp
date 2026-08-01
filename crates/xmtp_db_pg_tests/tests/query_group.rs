//! The sqlx `QueryGroup` impl, method by method.
//!
//! `find_groups` gets the most attention: it is the only query in the trait
//! whose SQL is assembled at runtime, and the two places its Postgres form had
//! to diverge from the sync path -- `encode(id, 'hex')` inside the DM-stitching
//! `COALESCE`, and the consent join's three shapes -- are exactly the places a
//! silent wrong answer would hide.

use xmtp_db::consent_record::{ConsentState, ConsentType};
use xmtp_db::group::{
    ConversationType, GroupMembershipState, GroupQueryArgs, GroupQueryOrderBy, QueryGroup,
    StoredGroup,
};
use xmtp_db::pg::PgDb;
use xmtp_db::{DuplicateItem, StorageError};
use xmtp_db_pg_tests::fresh_db;
use xmtp_proto::types::{Cursor, GroupId};

fn gid(n: u8) -> GroupId {
    GroupId::from([n; 16])
}

fn group(id: GroupId, created_at_ns: i64) -> StoredGroup {
    StoredGroup::builder()
        .id(id)
        .created_at_ns(created_at_ns)
        .membership_state(GroupMembershipState::Allowed)
        .added_by_inbox_id("inbox")
        .build()
        .unwrap()
}

fn dm(id: GroupId, dm_id: &str, last_message_ns: Option<i64>) -> StoredGroup {
    StoredGroup::builder()
        .id(id)
        .created_at_ns(0)
        .membership_state(GroupMembershipState::Allowed)
        .added_by_inbox_id("inbox")
        .dm_id(Some(dm_id.to_string()))
        .last_message_ns(last_message_ns)
        .build()
        .unwrap()
}

fn virtual_group(id: GroupId, kind: ConversationType) -> StoredGroup {
    StoredGroup::builder()
        .id(id)
        .created_at_ns(0)
        .membership_state(GroupMembershipState::Allowed)
        .added_by_inbox_id("inbox")
        .conversation_type(kind)
        .build()
        .unwrap()
}

async fn store(db: &PgDb, group: StoredGroup) -> StoredGroup {
    db.insert_or_replace_group(group).await.unwrap()
}

async fn set_consent(db: &PgDb, id: &GroupId, state: ConsentState) {
    let mut c = db.conn().await.unwrap();
    sqlx::query(
        "INSERT INTO consent_records (entity_type, state, entity, consented_at_ns) \
         VALUES ($1, $2, $3, 0)",
    )
    .bind(ConsentType::ConversationId)
    .bind(state)
    .bind(hex::encode(id.as_slice()))
    .execute(&mut *c)
    .await
    .unwrap();
}

fn ids(groups: &[StoredGroup]) -> Vec<GroupId> {
    groups.iter().map(|g| g.id).collect()
}

// --- find_groups ------------------------------------------------------------

#[tokio::test]
async fn find_groups_excludes_virtual_conversation_types() {
    let db = fresh_db("g_virtual").await;
    store(&db, group(gid(1), 1)).await;
    store(&db, virtual_group(gid(2), ConversationType::Sync)).await;
    store(&db, virtual_group(gid(3), ConversationType::Oneshot)).await;

    let found = db.find_groups(GroupQueryArgs::default()).await.unwrap();
    assert_eq!(ids(&found), vec![gid(1)]);
}

/// The stitching key is `COALESCE(dm_id, encode(id, 'hex'))`: DMs collapse to
/// the most recently active row, non-DMs only ever match themselves.
#[tokio::test]
async fn find_groups_keeps_only_the_latest_row_per_dm() {
    let db = fresh_db("g_dm_dedup").await;
    store(&db, dm(gid(1), "dm:a:b", Some(100))).await;
    store(&db, dm(gid(2), "dm:a:b", Some(300))).await;
    store(&db, dm(gid(3), "dm:a:b", Some(200))).await;
    store(&db, dm(gid(4), "dm:c:d", None)).await;
    store(&db, group(gid(5), 1)).await;

    let found = db.find_groups(GroupQueryArgs::default()).await.unwrap();
    let mut found = ids(&found);
    found.sort();
    assert_eq!(found, vec![gid(2), gid(4), gid(5)]);

    let all = db
        .find_groups(GroupQueryArgs {
            include_duplicate_dms: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(all.len(), 5);
}

/// A DM that has never been used has a NULL `last_message_ns`. The dedup
/// comparison `COALESCE(last_message_ns, 0)` must treat that as oldest, not --
/// as a bare Postgres DESC ordering would -- as newest.
#[tokio::test]
async fn find_groups_dedup_treats_a_null_last_message_as_oldest() {
    let db = fresh_db("g_dm_null").await;
    store(&db, dm(gid(1), "dm:a:b", None)).await;
    store(&db, dm(gid(2), "dm:a:b", Some(5))).await;

    let found = db.find_groups(GroupQueryArgs::default()).await.unwrap();
    assert_eq!(ids(&found), vec![gid(2)]);
}

#[tokio::test]
async fn find_groups_orders_by_created_at_or_last_activity() {
    let db = fresh_db("g_order").await;
    // Created oldest, but most recently active.
    store(&db, {
        let mut g = group(gid(1), 10);
        g.last_message_ns = Some(900);
        g
    })
    .await;
    store(&db, group(gid(2), 20)).await;
    store(&db, {
        let mut g = group(gid(3), 30);
        g.last_message_ns = Some(40);
        g
    })
    .await;

    let by_created = db
        .find_groups(GroupQueryArgs {
            order_by: Some(GroupQueryOrderBy::CreatedAt),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(ids(&by_created), vec![gid(1), gid(2), gid(3)]);

    // COALESCE(last_message_ns, created_at_ns) DESC -> 900, 40, 20.
    let by_activity = db
        .find_groups(GroupQueryArgs {
            order_by: Some(GroupQueryOrderBy::LastActivity),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(ids(&by_activity), vec![gid(1), gid(3), gid(2)]);
}

/// The default consent set is `{Allowed, Unknown}`, which takes the LEFT JOIN
/// arm: a group with no consent record at all is kept, and only Denied is cut.
#[tokio::test]
async fn find_groups_default_consent_keeps_unknown_and_unrecorded() {
    let db = fresh_db("g_consent_def").await;
    store(&db, group(gid(1), 1)).await; // no consent record
    store(&db, group(gid(2), 2)).await;
    store(&db, group(gid(3), 3)).await;
    store(&db, group(gid(4), 4)).await;
    set_consent(&db, &gid(2), ConsentState::Allowed).await;
    set_consent(&db, &gid(3), ConsentState::Unknown).await;
    set_consent(&db, &gid(4), ConsentState::Denied).await;

    let found = db.find_groups(GroupQueryArgs::default()).await.unwrap();
    assert_eq!(ids(&found), vec![gid(1), gid(2), gid(3)]);
}

/// Without Unknown in the set the join becomes INNER, so a group with no consent
/// record drops out entirely.
#[tokio::test]
async fn find_groups_explicit_states_require_a_consent_record() {
    let db = fresh_db("g_consent_inner").await;
    store(&db, group(gid(1), 1)).await; // no consent record
    store(&db, group(gid(2), 2)).await;
    store(&db, group(gid(3), 3)).await;
    set_consent(&db, &gid(2), ConsentState::Denied).await;
    set_consent(&db, &gid(3), ConsentState::Allowed).await;

    let denied = db
        .find_groups(GroupQueryArgs {
            consent_states: Some(vec![ConsentState::Denied]),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(ids(&denied), vec![gid(2)]);
}

/// All three states means no filtering, so the join is skipped and a group with
/// no consent record is kept.
#[tokio::test]
async fn find_groups_all_consent_states_skips_the_join() {
    let db = fresh_db("g_consent_all").await;
    store(&db, group(gid(1), 1)).await;
    store(&db, group(gid(2), 2)).await;
    set_consent(&db, &gid(2), ConsentState::Denied).await;

    let found = db
        .find_groups(GroupQueryArgs {
            consent_states: Some(vec![
                ConsentState::Allowed,
                ConsentState::Denied,
                ConsentState::Unknown,
            ]),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(ids(&found), vec![gid(1), gid(2)]);
}

#[tokio::test]
async fn find_groups_applies_scalar_filters() {
    let db = fresh_db("g_filters").await;
    store(&db, group(gid(1), 10)).await;
    store(&db, group(gid(2), 20)).await;
    store(&db, {
        let mut g = group(gid(3), 30);
        g.membership_state = GroupMembershipState::Pending;
        g.should_publish_commit_log = true;
        g
    })
    .await;

    // created_after / created_before are both exclusive here.
    let window = db
        .find_groups(GroupQueryArgs {
            created_after_ns: Some(10),
            created_before_ns: Some(30),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(ids(&window), vec![gid(2)]);

    let pending = db
        .find_groups(GroupQueryArgs {
            allowed_states: Some(vec![GroupMembershipState::Pending]),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(ids(&pending), vec![gid(3)]);

    let publishers = db
        .find_groups(GroupQueryArgs {
            should_publish_commit_log: Some(true),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(ids(&publishers), vec![gid(3)]);

    let limited = db
        .find_groups(GroupQueryArgs {
            limit: Some(2),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(ids(&limited), vec![gid(1), gid(2)]);
}

/// The activity window is over `COALESCE(last_message_ns, created_at_ns)`, so a
/// group with no messages is judged by when it was created.
#[tokio::test]
async fn find_groups_activity_window_falls_back_to_created_at() {
    let db = fresh_db("g_activity").await;
    store(&db, group(gid(1), 100)).await;
    store(&db, {
        let mut g = group(gid(2), 10);
        g.last_message_ns = Some(500);
        g
    })
    .await;

    let recent = db
        .find_groups(GroupQueryArgs {
            last_activity_after_ns: Some(50),
            ..Default::default()
        })
        .await
        .unwrap();
    let mut recent = ids(&recent);
    recent.sort();
    assert_eq!(recent, vec![gid(1), gid(2)]);

    let old = db
        .find_groups(GroupQueryArgs {
            last_activity_before_ns: Some(200),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(ids(&old), vec![gid(1)]);
}

#[tokio::test]
async fn find_groups_appends_sync_groups_only_when_asked() {
    let db = fresh_db("g_sync_append").await;
    store(&db, group(gid(1), 1)).await;
    store(&db, virtual_group(gid(2), ConversationType::Sync)).await;

    let with_sync = db
        .find_groups(GroupQueryArgs {
            include_sync_groups: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(ids(&with_sync), vec![gid(1), gid(2)]);

    // Asking for the Sync type alone returns only the sync groups: the main
    // query filters every virtual type out.
    let only_sync = db
        .find_groups(GroupQueryArgs {
            conversation_type: Some(ConversationType::Sync),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(ids(&only_sync), vec![gid(2)]);
}

#[tokio::test]
async fn find_groups_rejects_conflicting_time_filters() {
    let db = fresh_db("g_conflict").await;
    let err = db
        .find_groups(GroupQueryArgs {
            created_after_ns: Some(1),
            last_activity_after_ns: Some(1),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert!(matches!(err, xmtp_db::ConnectionError::InvalidQuery(_)));
}

#[tokio::test]
async fn find_groups_by_id_paged_orders_by_id_and_offsets() {
    let db = fresh_db("g_paged").await;
    for n in 1..=5u8 {
        store(&db, group(gid(n), n as i64 * 10)).await;
    }
    store(&db, virtual_group(gid(9), ConversationType::Sync)).await;

    let page = db
        .find_groups_by_id_paged(
            GroupQueryArgs {
                limit: Some(2),
                ..Default::default()
            },
            1,
        )
        .await
        .unwrap();
    assert_eq!(ids(&page), vec![gid(2), gid(3)]);

    // Unlike find_groups, the upper bound here is inclusive.
    let bounded = db
        .find_groups_by_id_paged(
            GroupQueryArgs {
                created_before_ns: Some(20),
                ..Default::default()
            },
            0,
        )
        .await
        .unwrap();
    assert_eq!(ids(&bounded), vec![gid(1), gid(2)]);
}

// --- insert_or_replace_group ------------------------------------------------

#[tokio::test]
async fn insert_or_replace_group_returns_the_stored_row() {
    let db = fresh_db("g_insert").await;
    let stored = store(&db, group(gid(1), 42)).await;
    assert_eq!(stored.id, gid(1));
    assert_eq!(stored.created_at_ns, 42);
    // Column defaults come back through RETURNING.
    assert_eq!(stored.fork_details, "");
    assert!(!stored.maybe_forked);
}

/// Re-inserting a welcome that already produced a group must fail, so the
/// caller's openmls transaction rolls back rather than double-processing it.
#[tokio::test]
async fn insert_or_replace_group_rejects_a_duplicate_welcome() {
    let db = fresh_db("g_dup_welcome").await;
    let mut g = group(gid(1), 1);
    g.sequence_id = Some(7);
    g.originator_id = Some(3);
    store(&db, g.clone()).await;

    let err = db.insert_or_replace_group(g).await.unwrap_err();
    assert!(matches!(
        err,
        StorageError::Duplicate(DuplicateItem::WelcomeId(_))
    ));
}

#[tokio::test]
async fn insert_or_replace_group_advances_sequence_and_originator_together() {
    let db = fresh_db("g_advance").await;
    store(&db, group(gid(1), 1)).await;

    let mut newer = group(gid(1), 1);
    newer.sequence_id = Some(9);
    newer.originator_id = Some(4);
    let returned = db.insert_or_replace_group(newer).await.unwrap();
    assert_eq!(returned.sequence_id, Some(9));
    assert_eq!(returned.originator_id, Some(4));

    let reloaded = db.find_group(&gid(1)).await.unwrap().unwrap();
    assert_eq!(reloaded.cursor(), Some(Cursor::new(9, 4u32)));

    // A lower sequence id must not roll the cursor backwards.
    let mut older = group(gid(1), 1);
    older.sequence_id = Some(2);
    older.originator_id = Some(4);
    db.insert_or_replace_group(older).await.unwrap();
    let reloaded = db.find_group(&gid(1)).await.unwrap().unwrap();
    assert_eq!(reloaded.sequence_id, Some(9));
}

/// A restored group is overwritten by the incoming one -- but only field by
/// field: diesel's `AsChangeset` skips `None` options, and the Postgres impl
/// reproduces that with `COALESCE(new, existing)`.
#[tokio::test]
async fn insert_or_replace_group_overwrites_restored_but_keeps_unset_fields() {
    let db = fresh_db("g_restored").await;
    let mut restored = group(gid(1), 1);
    restored.membership_state = GroupMembershipState::Restored;
    restored.dm_id = Some("dm:a:b".to_string());
    restored.paused_for_version = Some("1.2.3".to_string());
    restored.conversation_type = ConversationType::Dm;
    store(&db, restored).await;

    let mut incoming = group(gid(1), 55);
    incoming.added_by_inbox_id = "new-inbox".to_string();
    incoming.conversation_type = ConversationType::Dm;
    incoming.sequence_id = Some(4);
    incoming.originator_id = Some(1);
    db.insert_or_replace_group(incoming).await.unwrap();

    let reloaded = db.find_group(&gid(1)).await.unwrap().unwrap();
    assert_eq!(reloaded.membership_state, GroupMembershipState::Allowed);
    assert_eq!(reloaded.added_by_inbox_id, "new-inbox");
    assert_eq!(reloaded.created_at_ns, 55);
    // Untouched because the incoming group left them None.
    assert_eq!(reloaded.dm_id.as_deref(), Some("dm:a:b"));
    assert_eq!(reloaded.paused_for_version.as_deref(), Some("1.2.3"));
}

// --- cursors ----------------------------------------------------------------

#[tokio::test]
async fn group_cursors_skips_rows_with_a_null_originator() {
    let db = fresh_db("g_cursors").await;
    let mut complete = group(gid(1), 1);
    complete.sequence_id = Some(5);
    complete.originator_id = Some(2);
    store(&db, complete).await;
    store(&db, group(gid(2), 2)).await; // no cursor at all

    // The builder refuses this pairing, so the half-populated row -- the shape a
    // pre-fix client could have written -- has to be made directly.
    let mut c = db.conn().await.unwrap();
    sqlx::query(
        "INSERT INTO groups (id, created_at_ns, membership_state, installations_last_checked, \
         added_by_inbox_id, sequence_id) VALUES ($1, 0, 1, 0, '', 8)",
    )
    .bind(gid(3))
    .execute(&mut *c)
    .await
    .unwrap();
    drop(c);

    let cursors = db.group_cursors().await.unwrap();
    assert_eq!(cursors, vec![Cursor::new(5, 2u32)]);
}

// --- lookups ----------------------------------------------------------------

#[tokio::test]
async fn sync_group_lookups() {
    let db = fresh_db("g_sync_lookup").await;
    let mut older = virtual_group(gid(1), ConversationType::Sync);
    older.created_at_ns = 10;
    store(&db, older).await;
    let mut newer = virtual_group(gid(2), ConversationType::Sync);
    newer.created_at_ns = 20;
    store(&db, newer).await;
    store(&db, group(gid(3), 30)).await;

    let all = db.all_sync_groups().await.unwrap();
    assert_eq!(ids(&all), vec![gid(2), gid(1)], "newest first");

    let primary = db.primary_sync_group().await.unwrap().unwrap();
    assert_eq!(primary.id, gid(2));

    assert!(db.find_sync_group(&gid(1)).await.unwrap().is_some());
    assert!(
        db.find_sync_group(&gid(3)).await.unwrap().is_none(),
        "a regular group is not a sync group"
    );
}

#[tokio::test]
async fn find_group_by_sequence_id_matches_the_whole_cursor() {
    let db = fresh_db("g_by_seq").await;
    let mut g = group(gid(1), 1);
    g.sequence_id = Some(5);
    g.originator_id = Some(2);
    store(&db, g).await;

    let found = db
        .find_group_by_sequence_id(Cursor::new(5, 2u32))
        .await
        .unwrap();
    assert_eq!(found.map(|g| g.id), Some(gid(1)));

    // Same sequence id, different originator.
    assert!(
        db.find_group_by_sequence_id(Cursor::new(5, 9u32))
            .await
            .unwrap()
            .is_none()
    );
    assert!(db.find_group(&gid(2)).await.unwrap().is_none());
}

#[tokio::test]
async fn get_conversation_type_reads_the_column() {
    let db = fresh_db("g_conv_type").await;
    store(&db, virtual_group(gid(1), ConversationType::Oneshot)).await;
    assert_eq!(
        db.get_conversation_type(&gid(1)).await.unwrap(),
        ConversationType::Oneshot
    );
}

// --- timestamps and flags ---------------------------------------------------

#[tokio::test]
async fn rotation_and_installation_timestamps_roundtrip() {
    let db = fresh_db("g_timestamps").await;
    store(&db, group(gid(1), 1)).await;

    assert_eq!(db.get_rotated_at_ns(&gid(1)).await.unwrap(), 0);
    db.update_rotated_at_ns(&gid(1)).await.unwrap();
    assert!(db.get_rotated_at_ns(&gid(1)).await.unwrap() > 0);

    assert_eq!(db.get_installations_time_checked(&gid(1)).await.unwrap(), 0);
    db.update_installations_time_checked(&gid(1)).await.unwrap();
    assert!(db.get_installations_time_checked(&gid(1)).await.unwrap() > 0);

    // A missing group is NotFound, not a zero.
    assert!(matches!(
        db.get_rotated_at_ns(&gid(2)).await.unwrap_err(),
        StorageError::NotFound(_)
    ));
    assert!(matches!(
        db.get_installations_time_checked(&gid(2))
            .await
            .unwrap_err(),
        StorageError::NotFound(_)
    ));
}

#[tokio::test]
async fn membership_and_disappearing_settings_update() {
    let db = fresh_db("g_updates").await;
    store(&db, group(gid(1), 1)).await;

    db.update_group_membership(gid(1), GroupMembershipState::Rejected)
        .await
        .unwrap();
    db.update_message_disappearing_from_ns(&gid(1), Some(10))
        .await
        .unwrap();
    db.update_message_disappearing_in_ns(&gid(1), Some(20))
        .await
        .unwrap();

    let g = db.find_group(&gid(1)).await.unwrap().unwrap();
    assert_eq!(g.membership_state, GroupMembershipState::Rejected);
    assert_eq!(g.message_disappear_from_ns, Some(10));
    assert_eq!(g.message_disappear_in_ns, Some(20));

    // Clearing back to NULL is a real update, not a skipped one.
    db.update_message_disappearing_from_ns(&gid(1), None)
        .await
        .unwrap();
    let g = db.find_group(&gid(1)).await.unwrap().unwrap();
    assert_eq!(g.message_disappear_from_ns, None);
}

#[tokio::test]
async fn fork_flags_set_and_clear() {
    let db = fresh_db("g_fork_flags").await;
    store(&db, group(gid(1), 1)).await;

    db.mark_group_as_maybe_forked(&gid(1), "epoch mismatch".to_string())
        .await
        .unwrap();
    let g = db.find_group(&gid(1)).await.unwrap().unwrap();
    assert!(g.maybe_forked);
    assert_eq!(g.fork_details, "epoch mismatch");

    db.clear_fork_flag_for_group(&gid(1)).await.unwrap();
    let g = db.find_group(&gid(1)).await.unwrap().unwrap();
    assert!(!g.maybe_forked);
    assert_eq!(g.fork_details, "");

    assert_eq!(
        db.get_group_commit_log_forked_status(&gid(1))
            .await
            .unwrap(),
        None
    );
    db.set_group_commit_log_forked_status(&gid(1), Some(true))
        .await
        .unwrap();
    assert_eq!(
        db.get_group_commit_log_forked_status(&gid(1))
            .await
            .unwrap(),
        Some(true)
    );
}

/// The commit log's consensus key is derived once from the log's first entry, so
/// a second write is a conflict rather than an update.
#[tokio::test]
async fn commit_log_public_key_is_write_once() {
    let db = fresh_db("g_clpk").await;
    store(&db, group(gid(1), 1)).await;

    db.set_group_commit_log_public_key(&gid(1), &[1, 2, 3])
        .await
        .unwrap();
    let err = db
        .set_group_commit_log_public_key(&gid(1), &[4, 5, 6])
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        StorageError::Duplicate(DuplicateItem::CommitLogPublicKey(_))
    ));

    let g = db.find_group(&gid(1)).await.unwrap().unwrap();
    assert_eq!(g.commit_log_public_key, Some(vec![1, 2, 3]));

    // A group that does not exist updates nothing, which reports the same way.
    assert!(
        db.set_group_commit_log_public_key(&gid(2), &[7])
            .await
            .is_err()
    );
}

#[tokio::test]
async fn has_duplicate_dm_counts_rows_sharing_a_dm_id() {
    let db = fresh_db("g_dup_dm").await;
    store(&db, dm(gid(1), "dm:a:b", Some(1))).await;
    store(&db, dm(gid(2), "dm:a:b", Some(2))).await;
    store(&db, dm(gid(3), "dm:c:d", Some(3))).await;
    store(&db, group(gid(4), 1)).await;

    assert!(db.has_duplicate_dm(&gid(1)).await.unwrap());
    assert!(!db.has_duplicate_dm(&gid(3)).await.unwrap());
    assert!(!db.has_duplicate_dm(&gid(4)).await.unwrap(), "not a DM");
    assert!(
        !db.has_duplicate_dm(&gid(9)).await.unwrap(),
        "unknown group is not a duplicate"
    );
}

#[tokio::test]
async fn groups_with_a_pending_leave_request() {
    let db = fresh_db("g_pending_leave").await;
    store(&db, group(gid(1), 1)).await;
    store(&db, group(gid(2), 2)).await;
    store(&db, virtual_group(gid(3), ConversationType::Sync)).await;

    db.set_group_has_pending_leave_request_status(&gid(1), Some(true))
        .await
        .unwrap();
    db.set_group_has_pending_leave_request_status(&gid(2), Some(false))
        .await
        .unwrap();
    db.set_group_has_pending_leave_request_status(&gid(3), Some(true))
        .await
        .unwrap();

    let pending = db.get_groups_have_pending_leave_request().await.unwrap();
    assert_eq!(pending, vec![gid(1).to_vec()], "sync groups are excluded");
}

// --- commit-log listings ----------------------------------------------------

#[tokio::test]
async fn conversation_ids_for_remote_log_publish_and_download() {
    let db = fresh_db("g_log_lists").await;
    store(&db, dm(gid(1), "dm:a:b", Some(1))).await;
    store(&db, {
        let mut g = group(gid(2), 2);
        g.should_publish_commit_log = true;
        g
    })
    .await;
    store(&db, group(gid(3), 3)).await; // group, not publishing
    store(&db, virtual_group(gid(4), ConversationType::Sync)).await;
    for n in 1..=4u8 {
        set_consent(&db, &gid(n), ConsentState::Allowed).await;
    }
    // Allowed consent is required on both listings.
    store(&db, dm(gid(5), "dm:e:f", Some(5))).await;
    set_consent(&db, &gid(5), ConsentState::Denied).await;

    let publish = db
        .get_conversation_ids_for_remote_log_publish()
        .await
        .unwrap();
    assert_eq!(
        publish.iter().map(|g| g.id).collect::<Vec<_>>(),
        vec![gid(1), gid(2)]
    );

    let mut download = db
        .get_conversation_ids_for_remote_log_download()
        .await
        .unwrap()
        .iter()
        .map(|g| g.id)
        .collect::<Vec<_>>();
    download.sort();
    assert_eq!(
        download,
        vec![gid(1), gid(2), gid(3)],
        "every non-virtual allowed conversation, publishing or not"
    );
}

#[tokio::test]
async fn conversation_ids_for_fork_check_skips_known_forks() {
    let db = fresh_db("g_fork_check").await;
    store(&db, group(gid(1), 1)).await; // NULL status
    store(&db, group(gid(2), 2)).await;
    store(&db, group(gid(3), 3)).await;
    store(&db, virtual_group(gid(4), ConversationType::Sync)).await;
    db.set_group_commit_log_forked_status(&gid(2), Some(false))
        .await
        .unwrap();
    db.set_group_commit_log_forked_status(&gid(3), Some(true))
        .await
        .unwrap();

    let mut checkable = db.get_conversation_ids_for_fork_check().await.unwrap();
    checkable.sort();
    assert_eq!(checkable, vec![gid(1).to_vec(), gid(2).to_vec()]);
}

#[tokio::test]
async fn conversation_ids_for_requesting_readds_carry_the_latest_commit() {
    let db = fresh_db("g_req_readds").await;
    store(&db, group(gid(1), 1)).await;
    store(&db, group(gid(2), 2)).await; // forked, no remote log yet
    store(&db, group(gid(3), 3)).await; // not forked
    db.set_group_commit_log_forked_status(&gid(1), Some(true))
        .await
        .unwrap();
    db.set_group_commit_log_forked_status(&gid(2), Some(true))
        .await
        .unwrap();

    let mut c = db.conn().await.unwrap();
    for (log_seq, commit_seq) in [(1i64, 10i64), (2, 30), (3, 20)] {
        sqlx::query(
            "INSERT INTO remote_commit_log \
             (log_sequence_id, group_id, commit_sequence_id, commit_result, \
              applied_epoch_number, applied_epoch_authenticator) \
             VALUES ($1, $2, $3, 1, 0, '\\x00')",
        )
        .bind(log_seq)
        .bind(gid(1))
        .bind(commit_seq)
        .execute(&mut *c)
        .await
        .unwrap();
    }
    drop(c);

    let mut readds = db
        .get_conversation_ids_for_requesting_readds()
        .await
        .unwrap();
    readds.sort_by_key(|r| r.group_id);
    assert_eq!(readds.len(), 2);
    assert_eq!(readds[0].group_id, gid(1));
    assert_eq!(readds[0].latest_commit_sequence_id, Some(30));
    assert_eq!(readds[1].group_id, gid(2));
    assert_eq!(
        readds[1].latest_commit_sequence_id, None,
        "LEFT JOIN keeps a forked group with no remote log"
    );
}

#[tokio::test]
async fn conversation_ids_for_responding_readds() {
    let db = fresh_db("g_resp_readds").await;
    for n in 1..=4u8 {
        store(&db, group(gid(n), n as i64)).await;
    }

    let mut c = db.conn().await.unwrap();
    // requested / responded pairs: unanswered, stale answer, current answer, none.
    for (id, requested, responded) in [
        (gid(1), Some(5i64), None),
        (gid(2), Some(5), Some(3)),
        (gid(3), Some(5), Some(9)),
        (gid(4), None, None),
    ] {
        sqlx::query(
            "INSERT INTO readd_status \
             (group_id, installation_id, requested_at_sequence_id, responded_at_sequence_id) \
             VALUES ($1, '\\x01', $2, $3)",
        )
        .bind(id)
        .bind(requested)
        .bind(responded)
        .execute(&mut *c)
        .await
        .unwrap();
    }
    drop(c);

    let mut responding = db
        .get_conversation_ids_for_responding_readds()
        .await
        .unwrap();
    responding.sort_by_key(|g| g.group_id);
    assert_eq!(
        responding.iter().map(|g| g.group_id).collect::<Vec<_>>(),
        vec![gid(1), gid(2)]
    );
    assert_eq!(responding[0].created_at_ns, 1);
    assert_eq!(responding[0].conversation_type, ConversationType::Group);
    assert_eq!(responding[0].dm_id, None);
}
