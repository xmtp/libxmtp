//! `QueryGroupVersion` over sqlx/Postgres, against the committed schema.

use xmtp_db::group::QueryGroupVersion;
use xmtp_db::pg::PgDb;
use xmtp_db_pg_tests::fresh_db;
use xmtp_proto::types::GroupId;

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

#[tokio::test]
async fn set_and_read_paused_version() {
    let db = fresh_db("gv_set_read").await;
    let id = GroupId::ONE;
    insert_group(&db, &id).await;

    assert_eq!(
        db.get_group_paused_version(&id).await.unwrap(),
        None,
        "a group with no pause set reads back as None"
    );

    db.set_group_paused(&id, "1.2.3").await.unwrap();
    assert_eq!(
        db.get_group_paused_version(&id).await.unwrap(),
        Some("1.2.3".to_string())
    );
}

#[tokio::test]
async fn unpause_clears_the_version() {
    let db = fresh_db("gv_unpause").await;
    let id = GroupId::TWO;
    insert_group(&db, &id).await;

    db.set_group_paused(&id, "9.9.9").await.unwrap();
    db.unpause_group(&id).await.unwrap();

    assert_eq!(db.get_group_paused_version(&id).await.unwrap(), None);
}

/// A *missing* group row is an error, not `Ok(None)`. This mirrors the diesel
/// impl's `first()`, where `None` means the row exists with a null
/// `paused_for_version`. Collapsing the two would silently change behavior for
/// callers that distinguish them.
#[tokio::test]
async fn missing_group_is_an_error_not_none() {
    let db = fresh_db("gv_missing").await;
    let res = db.get_group_paused_version(&GroupId::THREE).await;
    assert!(
        res.is_err(),
        "querying a nonexistent group must error, got {res:?}"
    );
}

#[tokio::test]
async fn lists_only_paused_groups() {
    let db = fresh_db("gv_list").await;
    let paused = GroupId::ONE;
    let unpaused = GroupId::TWO;
    insert_group(&db, &paused).await;
    insert_group(&db, &unpaused).await;

    db.set_group_paused(&paused, "4.5.6").await.unwrap();

    let listed = db.get_paused_groups_with_versions().await.unwrap();
    assert_eq!(listed, vec![(paused, "4.5.6".to_string())]);
}

/// The 16-byte `GroupId` invariant is enforced on decode. A row whose id is the
/// wrong width is skipped with a warning rather than truncated or panicking --
/// same as the diesel path.
#[tokio::test]
async fn rows_with_malformed_group_ids_are_skipped() {
    let db = fresh_db("gv_malformed").await;
    let good = GroupId::ONE;
    insert_group(&db, &good).await;
    db.set_group_paused(&good, "1.0.0").await.unwrap();

    // Insert a paused row whose id is 8 bytes, which `GroupId` must reject.
    {
        let mut c = db.conn().await.unwrap();
        sqlx::query(
            "INSERT INTO groups (id, created_at_ns, membership_state, \
             installations_last_checked, added_by_inbox_id, paused_for_version) \
             VALUES ($1, 0, 1, 0, '', '2.0.0')",
        )
        .bind(vec![1u8; 8])
        .execute(&mut *c)
        .await
        .unwrap();
    }

    let listed = db.get_paused_groups_with_versions().await.unwrap();
    assert_eq!(
        listed,
        vec![(good, "1.0.0".to_string())],
        "the 8-byte id row must be skipped, not truncated into a GroupId"
    );
}
