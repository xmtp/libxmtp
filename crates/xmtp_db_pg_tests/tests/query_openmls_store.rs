//! The OpenMLS Postgres tables in libxmtp's async schema.
//!
//! `openmls_pg_storage` (the per-type typed tables) and `PgKeyStore` (the
//! generic `openmls_key_value` KV) read and write these tables; nothing in the
//! rest of the xmtp_db query suite touches them. That blind spot is exactly how
//! the schema once shipped the sync/SQLite `openmls_key_store`/`openmls_key_value`
//! KV tables instead of `openmls_pg_storage`'s per-type tables — unnoticed until
//! a real client registration hit `relation "openmls_signature_key" does not
//! exist`. These tests close that gap.

use sqlx::Row;
use xmtp_db::PgKeyStore;
use xmtp_db::XmtpMlsStorageProvider;
use xmtp_db::pg::PgDb;
use xmtp_db_pg_tests::fresh_db;

/// `count(*)` for a table in the fresh_db's schema (search_path is set per
/// connection by the harness, so an unqualified name resolves there).
async fn count(db: &PgDb, table: &str) -> i64 {
    let mut c = db.conn().await.expect("connection");
    let row = sqlx::query(&format!("SELECT count(*) AS n FROM {table}"))
        .fetch_one(&mut *c)
        .await
        .unwrap_or_else(|e| panic!("counting {table}: {e}"));
    row.get::<i64, _>("n")
}

/// Whether a table is present in the connection's schema (`to_regclass`
/// resolves against search_path and yields NULL when absent).
async fn table_exists(db: &PgDb, table: &str) -> bool {
    let mut c = db.conn().await.expect("connection");
    let row = sqlx::query("SELECT to_regclass($1) IS NOT NULL AS present")
        .bind(table)
        .fetch_one(&mut *c)
        .await
        .expect("to_regclass");
    row.get::<bool, _>("present")
}

/// libxmtp's `XmtpMlsStorageProvider` KV (key-package references, the commit-log
/// signer key, …) round-trips through the Postgres `openmls_key_value` table.
/// This is the exact path that returned `UnsupportedMethod` before the async KV
/// was implemented, which broke client registration.
#[tokio::test]
async fn kv_round_trip_through_openmls_key_value() {
    let db = fresh_db("openmls_kv_round_trip").await;
    let store = PgKeyStore::new(db.clone());

    let value: Vec<u8> = vec![9, 8, 7, 6, 5, 4, 3, 2, 1];
    let serialized = bincode::serialize(&value).expect("serialize");

    // Absent → None.
    let before: Option<Vec<u8>> = store.read::<Vec<u8>>(b"kv_label", b"k1").await.unwrap();
    assert_eq!(before, None);

    // Write → one row in openmls_key_value.
    store.write(b"kv_label", b"k1", &serialized).await.unwrap();
    assert_eq!(count(&db, "openmls_key_value").await, 1);

    // Read the typed value back.
    let got: Option<Vec<u8>> = store.read::<Vec<u8>>(b"kv_label", b"k1").await.unwrap();
    assert_eq!(got, Some(value));

    // Re-writing the same (label, key) is an upsert, not a second row.
    let v2 = bincode::serialize(&vec![42u8, 43u8]).unwrap();
    store.write(b"kv_label", b"k1", &v2).await.unwrap();
    assert_eq!(count(&db, "openmls_key_value").await, 1);
    let got2: Option<Vec<u8>> = store.read::<Vec<u8>>(b"kv_label", b"k1").await.unwrap();
    assert_eq!(got2, Some(vec![42u8, 43u8]));

    // A different (label, key) is a distinct row.
    store.write(b"kv_label", b"k2", &serialized).await.unwrap();
    assert_eq!(count(&db, "openmls_key_value").await, 2);

    // Delete removes only its own row.
    store.delete(b"kv_label", b"k1").await.unwrap();
    assert_eq!(count(&db, "openmls_key_value").await, 1);
    let gone: Option<Vec<u8>> = store.read::<Vec<u8>>(b"kv_label", b"k1").await.unwrap();
    assert_eq!(gone, None);
}

/// The async schema installs `openmls_pg_storage`'s per-type tables (and NOT the
/// sync/SQLite `openmls_key_store`). Each typed table accepts the exact INSERT
/// its `StorageProvider` runs — a raw round-trip that fails loudly if a column
/// name/type ever drifts from what `openmls_pg_storage` writes.
#[tokio::test]
async fn typed_openmls_tables_match_the_storage_provider() {
    let db = fresh_db("openmls_typed_tables").await;

    // The generic KV table exists; the sync/SQLite key store does NOT.
    assert!(table_exists(&db, "openmls_key_value").await);
    assert!(
        !table_exists(&db, "openmls_key_store").await,
        "the sync/SQLite openmls_key_store must not be in the async schema"
    );

    let mut c = db.conn().await.unwrap();

    // openmls_group_data: (group_id, data_type, group_data); data_type is CHECKed.
    sqlx::query(
        "INSERT INTO openmls_group_data (group_id, data_type, group_data) VALUES ($1, $2, $3)",
    )
    .bind(vec![1u8; 16])
    .bind("tree")
    .bind(vec![0u8; 8])
    .execute(&mut *c)
    .await
    .expect("openmls_group_data insert");
    // `application_export_tree` is accepted (the later migration's CHECK value).
    sqlx::query(
        "INSERT INTO openmls_group_data (group_id, data_type, group_data) VALUES ($1, $2, $3)",
    )
    .bind(vec![2u8; 16])
    .bind("application_export_tree")
    .bind(vec![0u8; 8])
    .execute(&mut *c)
    .await
    .expect("application_export_tree must be an accepted data_type");

    sqlx::query("INSERT INTO openmls_proposal (group_id, proposal_ref, proposal) VALUES ($1, $2, $3)")
        .bind(vec![1u8; 16])
        .bind(vec![1u8; 4])
        .bind(vec![0u8; 8])
        .execute(&mut *c)
        .await
        .expect("openmls_proposal insert");

    sqlx::query("INSERT INTO openmls_own_leaf_node (group_id, leaf_node) VALUES ($1, $2)")
        .bind(vec![1u8; 16])
        .bind(vec![0u8; 8])
        .execute(&mut *c)
        .await
        .expect("openmls_own_leaf_node insert");

    sqlx::query("INSERT INTO openmls_signature_key (public_key, signature_key) VALUES ($1, $2)")
        .bind(vec![3u8; 32])
        .bind(vec![0u8; 8])
        .execute(&mut *c)
        .await
        .expect("openmls_signature_key insert");

    sqlx::query("INSERT INTO openmls_encryption_key (public_key, key_pair) VALUES ($1, $2)")
        .bind(vec![4u8; 32])
        .bind(vec![0u8; 8])
        .execute(&mut *c)
        .await
        .expect("openmls_encryption_key insert");

    sqlx::query(
        "INSERT INTO openmls_epoch_key_pairs (group_id, epoch_id, leaf_index, key_pairs) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(vec![1u8; 16])
    .bind(vec![0u8; 8])
    .bind(0i64)
    .bind(vec![0u8; 8])
    .execute(&mut *c)
    .await
    .expect("openmls_epoch_key_pairs insert");

    sqlx::query("INSERT INTO openmls_key_package (key_package_ref, key_package) VALUES ($1, $2)")
        .bind(vec![5u8; 16])
        .bind(vec![0u8; 8])
        .execute(&mut *c)
        .await
        .expect("openmls_key_package insert");

    sqlx::query("INSERT INTO openmls_psk (psk_id, psk_bundle) VALUES ($1, $2)")
        .bind(vec![6u8; 16])
        .bind(vec![0u8; 8])
        .execute(&mut *c)
        .await
        .expect("openmls_psk insert");

    // Read one back to prove it is really stored, not just accepted.
    let row = sqlx::query("SELECT signature_key FROM openmls_signature_key WHERE public_key = $1")
        .bind(vec![3u8; 32])
        .fetch_one(&mut *c)
        .await
        .expect("openmls_signature_key select");
    assert_eq!(row.get::<Vec<u8>, _>("signature_key"), vec![0u8; 8]);

    // The group_data CHECK rejects an unknown data_type.
    let bad = sqlx::query(
        "INSERT INTO openmls_group_data (group_id, data_type, group_data) VALUES ($1, $2, $3)",
    )
    .bind(vec![9u8; 16])
    .bind("not_a_real_data_type")
    .bind(vec![0u8; 8])
    .execute(&mut *c)
    .await;
    assert!(
        bad.is_err(),
        "openmls_group_data CHECK must reject an unknown data_type"
    );
}
