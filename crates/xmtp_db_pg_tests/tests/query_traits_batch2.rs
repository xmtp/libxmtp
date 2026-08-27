//! sqlx `Query*` impls: identity cache, key packages, message deletions, the two
//! commit logs, and consent records.
//!
//! Same rationale as `query_traits_simple`: with no `diesel::table!` on this
//! track, running the SQL is the only thing that checks it against the schema.

use xmtp_db::consent_record::{ConsentState, ConsentType, QueryConsentRecord, StoredConsentRecord};
use xmtp_db::identity_cache::{QueryIdentityCache, StoredIdentityKind};
use xmtp_db::key_package_history::QueryKeyPackageHistory;
use xmtp_db::local_commit_log::{LocalCommitLogOrder, QueryLocalCommitLog};
use xmtp_db::message_deletion::QueryMessageDeletion;
use xmtp_db::pg::PgDb;
use xmtp_db::remote_commit_log::{CommitResult, QueryRemoteCommitLog, RemoteCommitLogOrder};
use xmtp_db_pg_tests::fresh_db;
use xmtp_proto::types::GroupId;

// --- identity_cache ---------------------------------------------------------

#[tokio::test]
async fn identity_cache_matches_identity_and_kind_as_a_pair() {
    let db = fresh_db("ic_pair").await;
    db.cache_inbox_id(StoredIdentityKind::Ethereum, "0xabc".into(), "inbox_eth")
        .await
        .unwrap();
    db.cache_inbox_id(StoredIdentityKind::Passkey, "pk1".into(), "inbox_pk")
        .await
        .unwrap();

    let found = db
        .fetch_cached_inbox_ids(&[("0xabc".into(), StoredIdentityKind::Ethereum)])
        .await
        .unwrap();
    assert_eq!(found.get("0xabc").map(String::as_str), Some("inbox_eth"));

    // Right identity, wrong kind: both values exist in the table but not as a
    // pair, so a column-wise filter would wrongly match here.
    let none = db
        .fetch_cached_inbox_ids(&[("0xabc".into(), StoredIdentityKind::Passkey)])
        .await
        .unwrap();
    assert!(none.is_empty(), "must match on (identity, kind) as a pair");

    let empty = db.fetch_cached_inbox_ids(&[]).await.unwrap();
    assert!(empty.is_empty(), "no identifiers must not load the table");
}

/// The SQLite backend uses a plain `store`, so re-caching the same identity is a
/// primary-key violation rather than a silent overwrite.
#[tokio::test]
async fn identity_cache_rejects_duplicates() {
    let db = fresh_db("ic_dup").await;
    db.cache_inbox_id(StoredIdentityKind::Ethereum, "0xdup".into(), "first")
        .await
        .unwrap();
    let err = db
        .cache_inbox_id(StoredIdentityKind::Ethereum, "0xdup".into(), "second")
        .await;
    assert!(err.is_err(), "duplicate cache entry must error");
}

// --- key_package_history ----------------------------------------------------

#[tokio::test]
async fn key_package_store_is_idempotent_and_returns_the_entry() {
    let db = fresh_db("kp_store").await;
    let first = db
        .store_key_package_history_entry(vec![1, 2, 3], Some(vec![9]))
        .await
        .unwrap();
    assert_eq!(first.key_package_hash_ref, vec![1, 2, 3]);
    assert_eq!(first.post_quantum_public_key, Some(vec![9]));
    assert_eq!(first.delete_at_ns, None);

    // Storing the same hash ref again returns the *existing* row rather than
    // erroring or creating a second one.
    let again = db
        .store_key_package_history_entry(vec![1, 2, 3], Some(vec![9]))
        .await
        .unwrap();
    assert_eq!(again.id, first.id);
}

#[tokio::test]
async fn key_package_marking_is_idempotent_and_bounded_by_id() {
    let db = fresh_db("kp_mark").await;
    let a = db
        .store_key_package_history_entry(vec![1], None)
        .await
        .unwrap();
    let b = db
        .store_key_package_history_entry(vec![2], None)
        .await
        .unwrap();
    let c = db
        .store_key_package_history_entry(vec![3], None)
        .await
        .unwrap();

    assert_eq!(db.min_key_package_delete_at_ns().await.unwrap(), None);

    db.mark_key_package_before_id_to_be_deleted(c.id)
        .await
        .unwrap();
    let marked = db.min_key_package_delete_at_ns().await.unwrap();
    assert!(marked.is_some(), "a and b are now marked");

    // Re-marking must not push the existing deadline out.
    db.mark_key_package_before_id_to_be_deleted(c.id)
        .await
        .unwrap();
    assert_eq!(
        db.min_key_package_delete_at_ns().await.unwrap(),
        marked,
        "already-marked entries keep their original deadline"
    );

    let before_c = db
        .find_key_package_history_entries_before_id(c.id)
        .await
        .unwrap();
    assert_eq!(before_c.len(), 2, "strictly less than: a and b, not c");

    // `delete_at_ns` is 24h out, so nothing is expired yet.
    assert!(db.get_expired_key_packages().await.unwrap().is_empty());

    db.delete_key_package_entry_with_id(a.id).await.unwrap();
    assert!(
        db.find_key_package_history_entry_by_hash_ref(vec![1])
            .await
            .is_err(),
        "a deleted entry is a not-found error, matching the SQLite backend's first()"
    );

    db.delete_key_package_history_up_to_id(b.id).await.unwrap();
    assert_eq!(
        db.find_key_package_history_entries_before_id(i32::MAX)
            .await
            .unwrap()
            .len(),
        1,
        "only c survives"
    );
}

#[tokio::test]
async fn expired_key_packages_are_those_already_due() {
    let db = fresh_db("kp_expired").await;
    let e = db
        .store_key_package_history_entry(vec![7], None)
        .await
        .unwrap();

    let mut c = db.conn().await.unwrap();
    sqlx::query("UPDATE key_package_history SET delete_at_ns = $1 WHERE id = $2")
        .bind(1i64)
        .bind(e.id)
        .execute(&mut *c)
        .await
        .unwrap();
    drop(c);

    let expired = db.get_expired_key_packages().await.unwrap();
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].id, e.id);
    assert_eq!(db.min_key_package_delete_at_ns().await.unwrap(), Some(1));
}

// --- message_deletion -------------------------------------------------------

async fn insert_group(db: &PgDb, id: &GroupId) {
    let mut c = db.conn().await.unwrap();
    sqlx::query(
        "INSERT INTO groups (id, created_at_ns, membership_state, \
         installations_last_checked, added_by_inbox_id) VALUES ($1, 0, 1, 0, '')",
    )
    .bind(id)
    .execute(&mut *c)
    .await
    .unwrap();
}

async fn insert_message(db: &PgDb, group_id: &GroupId, id: &[u8]) {
    let mut c = db.conn().await.unwrap();
    sqlx::query(
        "INSERT INTO group_messages (id, group_id, decrypted_message_bytes, sent_at_ns, \
         sender_installation_id, sender_inbox_id, authority_id, originator_id, sequence_id, \
         idempotency_key) VALUES ($1, $2, '\\x00', 0, '\\x00', '', '', 0, 0, '')",
    )
    .bind(id)
    .bind(group_id)
    .execute(&mut *c)
    .await
    .unwrap();
}

async fn insert_deletion(db: &PgDb, group_id: &GroupId, id: &[u8], deleted: &[u8]) {
    let mut c = db.conn().await.unwrap();
    sqlx::query(
        "INSERT INTO message_deletions (id, group_id, deleted_message_id, \
         deleted_by_inbox_id, is_super_admin_deletion, deleted_at_ns) \
         VALUES ($1, $2, $3, 'deleter', TRUE, 42)",
    )
    .bind(id)
    .bind(group_id)
    .bind(deleted)
    .execute(&mut *c)
    .await
    .unwrap();
}

#[tokio::test]
async fn message_deletions_are_queryable_by_both_ids() {
    let db = fresh_db("md_query").await;
    let group = GroupId::ONE;
    insert_group(&db, &group).await;
    // The deletion's own id is a FK into group_messages (the DeleteMessage).
    insert_message(&db, &group, b"del1").await;
    insert_deletion(&db, &group, b"del1", b"victim").await;

    let by_id = db.get_message_deletion(b"del1").await.unwrap().unwrap();
    assert_eq!(by_id.deleted_message_id, b"victim".to_vec());
    assert_eq!(by_id.deleted_by_inbox_id, "deleter");
    assert!(by_id.is_super_admin_deletion);
    assert_eq!(by_id.deleted_at_ns, 42);

    let by_victim = db
        .get_deletion_by_deleted_message_id(b"victim")
        .await
        .unwrap();
    assert_eq!(by_victim.map(|d| d.id), Some(b"del1".to_vec()));

    assert!(db.is_message_deleted(b"victim").await.unwrap());
    assert!(!db.is_message_deleted(b"other").await.unwrap());

    assert_eq!(db.get_group_deletions(&group).await.unwrap().len(), 1);
    assert!(
        db.get_group_deletions(&GroupId::TWO)
            .await
            .unwrap()
            .is_empty()
    );

    assert_eq!(
        db.get_deletions_for_messages(vec![b"victim".to_vec()])
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        db.get_deletions_for_messages(vec![])
            .await
            .unwrap()
            .is_empty(),
        "an empty id list must not load the table"
    );
    assert!(
        db.get_message_deletion(b"missing").await.unwrap().is_none(),
        "a missing deletion is Ok(None), not an error"
    );
}

// --- commit logs ------------------------------------------------------------

async fn insert_remote_log(db: &PgDb, group: &GroupId, log_seq: i64, commit_seq: i64) {
    let mut c = db.conn().await.unwrap();
    sqlx::query(
        "INSERT INTO remote_commit_log (log_sequence_id, group_id, commit_sequence_id, \
         commit_result, applied_epoch_number, applied_epoch_authenticator) \
         VALUES ($1, $2, $3, 1, 0, '\\x00')",
    )
    .bind(log_seq)
    .bind(group)
    .bind(commit_seq)
    .execute(&mut *c)
    .await
    .unwrap();
}

#[tokio::test]
async fn remote_commit_log_latest_and_cursor() {
    let db = fresh_db("rcl").await;
    let group = GroupId::ONE;
    insert_remote_log(&db, &group, 10, 100).await;
    insert_remote_log(&db, &group, 30, 300).await;
    insert_remote_log(&db, &group, 20, 200).await;
    // commit_sequence_id 0 is excluded from the cursor query but NOT from
    // `get_latest` — the two have deliberately different filters.
    insert_remote_log(&db, &group, 40, 0).await;

    let latest = db
        .get_latest_remote_log_for_group(&group)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        latest.log_sequence_id, 40,
        "latest is the highest log_sequence_id regardless of insertion order, and \
         unlike the cursor query it does not exclude commit_sequence_id = 0"
    );
    assert_eq!(latest.commit_result, CommitResult::Success);

    assert!(
        db.get_latest_remote_log_for_group(&GroupId::TWO)
            .await
            .unwrap()
            .is_none()
    );

    let asc = db
        .get_remote_commit_log_after_cursor(&group, 0, RemoteCommitLogOrder::AscendingByRowid)
        .await
        .unwrap();
    assert_eq!(
        asc.iter().map(|l| l.log_sequence_id).collect::<Vec<_>>(),
        vec![10, 30, 20],
        "ordered by rowid, and the commit_sequence_id = 0 row is excluded"
    );

    let desc = db
        .get_remote_commit_log_after_cursor(&group, 0, RemoteCommitLogOrder::DescendingByRowid)
        .await
        .unwrap();
    assert_eq!(
        desc.iter().map(|l| l.log_sequence_id).collect::<Vec<_>>(),
        vec![20, 30, 10]
    );

    let after = db
        .get_remote_commit_log_after_cursor(&group, 1, RemoteCommitLogOrder::AscendingByRowid)
        .await
        .unwrap();
    assert_eq!(after.len(), 2, "cursor is exclusive");

    assert!(
        db.get_remote_commit_log_after_cursor(
            &group,
            i32::MAX as i64 + 1,
            RemoteCommitLogOrder::AscendingByRowid
        )
        .await
        .is_err(),
        "a cursor past i32::MAX cannot name a row and must error"
    );
}

async fn insert_local_log(db: &PgDb, group: &GroupId, commit_seq: i64) {
    let mut c = db.conn().await.unwrap();
    sqlx::query(
        "INSERT INTO local_commit_log (group_id, commit_sequence_id, last_epoch_authenticator, \
         commit_result, applied_epoch_number, applied_epoch_authenticator) \
         VALUES ($1, $2, '\\x00', 1, 0, '\\x00')",
    )
    .bind(group)
    .bind(commit_seq)
    .execute(&mut *c)
    .await
    .unwrap();
}

#[tokio::test]
async fn local_commit_log_chain_start_and_cursor() {
    let db = fresh_db("lcl").await;
    let group = GroupId::ONE;
    insert_local_log(&db, &group, 0).await; // chain start
    insert_local_log(&db, &group, 100).await;
    insert_local_log(&db, &group, 200).await;

    let logs = db.get_group_logs(&group).await.unwrap();
    assert_eq!(
        logs.iter()
            .map(|l| l.commit_sequence_id)
            .collect::<Vec<_>>(),
        vec![0, 100, 200],
        "ascending by rowid"
    );
    assert_eq!(logs[0].commit_result, CommitResult::Success);
    assert_eq!(logs[0].error_message, None);

    let latest = db.get_latest_log_for_group(&group).await.unwrap().unwrap();
    assert_eq!(latest.commit_sequence_id, 200);

    let cursor = db.get_local_commit_log_cursor(&group).await.unwrap();
    assert_eq!(cursor, Some(latest.rowid));

    let chain_start = db.get_latest_chain_start_rowid(&group).await.unwrap();
    assert_eq!(
        chain_start,
        Some(logs[0].rowid),
        "chain start is the commit_sequence_id = 0 row"
    );

    let after = db
        .get_local_commit_log_after_cursor(&group, 0, LocalCommitLogOrder::AscendingByRowid)
        .await
        .unwrap();
    assert_eq!(
        after
            .iter()
            .map(|l| l.commit_sequence_id)
            .collect::<Vec<_>>(),
        vec![100, 200],
        "chain starts are never published, so they are excluded"
    );

    assert_eq!(
        db.get_local_commit_log_cursor(&GroupId::TWO).await.unwrap(),
        None
    );
    assert_eq!(
        db.get_latest_chain_start_rowid(&GroupId::TWO)
            .await
            .unwrap(),
        None
    );
}

// --- consent_record ---------------------------------------------------------

fn consent(entity: &str, state: ConsentState, at_ns: i64) -> StoredConsentRecord {
    StoredConsentRecord {
        entity_type: ConsentType::InboxId,
        state,
        entity: entity.to_string(),
        consented_at_ns: at_ns,
    }
}

#[tokio::test]
async fn insert_newer_consent_record_respects_timestamps() {
    let db = fresh_db("cr_newer").await;

    assert!(
        db.insert_newer_consent_record(consent("a", ConsentState::Allowed, 100))
            .await
            .unwrap(),
        "first write is new"
    );

    assert!(
        !db.insert_newer_consent_record(consent("a", ConsentState::Allowed, 200))
            .await
            .unwrap(),
        "same decision restated later is not a change"
    );

    assert!(
        !db.insert_newer_consent_record(consent("a", ConsentState::Denied, 50))
            .await
            .unwrap(),
        "an older decision must not overwrite a newer one"
    );
    assert_eq!(
        db.get_consent_record("a".into(), ConsentType::InboxId)
            .await
            .unwrap()
            .unwrap()
            .state,
        ConsentState::Allowed
    );

    assert!(
        db.insert_newer_consent_record(consent("a", ConsentState::Denied, 300))
            .await
            .unwrap(),
        "a newer decision replaces"
    );
    assert_eq!(
        db.get_consent_record("a".into(), ConsentType::InboxId)
            .await
            .unwrap()
            .unwrap()
            .state,
        ConsentState::Denied
    );
}

#[tokio::test]
async fn insert_or_replace_returns_only_what_changed() {
    let db = fresh_db("cr_replace").await;
    db.insert_newer_consent_record(consent("a", ConsentState::Allowed, 1))
        .await
        .unwrap();

    let changed = db
        .insert_or_replace_consent_records(&[
            consent("a", ConsentState::Allowed, 5), // unchanged decision
            consent("b", ConsentState::Denied, 5),  // new
        ])
        .await
        .unwrap();
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].entity, "b");

    let changed = db
        .insert_or_replace_consent_records(&[consent("a", ConsentState::Denied, 9)])
        .await
        .unwrap();
    assert_eq!(changed.len(), 1, "a state flip counts as changed");
    assert_eq!(
        db.get_consent_record("a".into(), ConsentType::InboxId)
            .await
            .unwrap()
            .unwrap()
            .state,
        ConsentState::Denied
    );

    assert!(
        db.insert_or_replace_consent_records(&[])
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn maybe_insert_reports_the_existing_record() {
    let db = fresh_db("cr_maybe").await;
    let first = consent("a", ConsentState::Allowed, 1);

    assert!(
        db.maybe_insert_consent_record_return_existing(&first)
            .await
            .unwrap()
            .is_none(),
        "None means it was inserted"
    );

    let existing = db
        .maybe_insert_consent_record_return_existing(&consent("a", ConsentState::Denied, 2))
        .await
        .unwrap()
        .expect("Some means a record was already there");
    assert_eq!(
        existing.state,
        ConsentState::Allowed,
        "the existing record is returned unmodified"
    );
}

#[tokio::test]
async fn consent_paging_and_dm_lookup() {
    let db = fresh_db("cr_paged").await;
    for e in ["c", "a", "b"] {
        db.insert_newer_consent_record(consent(e, ConsentState::Allowed, 1))
            .await
            .unwrap();
    }
    assert_eq!(db.consent_records().await.unwrap().len(), 3);

    let page = db.consent_records_paged(2, 0).await.unwrap();
    assert_eq!(
        page.iter().map(|r| r.entity.as_str()).collect::<Vec<_>>(),
        vec!["a", "b"],
        "ordered by (entity_type, entity)"
    );
    let page = db.consent_records_paged(2, 2).await.unwrap();
    assert_eq!(page.len(), 1);

    // The dm lookup joins through groups, keyed on the hex-encoded group id.
    let group = GroupId::ONE;
    let mut c = db.conn().await.unwrap();
    sqlx::query(
        "INSERT INTO groups (id, created_at_ns, membership_state, \
         installations_last_checked, added_by_inbox_id, dm_id) VALUES ($1, 0, 1, 0, '', 'dm:x')",
    )
    .bind(group)
    .execute(&mut *c)
    .await
    .unwrap();
    drop(c);

    db.insert_newer_consent_record(StoredConsentRecord {
        entity_type: ConsentType::ConversationId,
        state: ConsentState::Allowed,
        entity: hex::encode(group.as_slice()),
        consented_at_ns: 7,
    })
    .await
    .unwrap();

    let found = db.find_consent_by_dm_id("dm:x").await.unwrap();
    assert_eq!(found.len(), 1, "hex encoding must match what Rust writes");
    assert_eq!(found[0].entity, hex::encode(group.as_slice()));

    assert!(
        db.find_consent_by_dm_id("dm:none")
            .await
            .unwrap()
            .is_empty()
    );
}
