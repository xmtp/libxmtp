// The bare PgKeyStore alone doesn't instantiate the giant client future, but the
// harness lib does; keep the limit consistent across the crate's test binaries.
#![recursion_limit = "512"]
//! Direct coverage of `PgKeyStore`'s typed value accessors against their
//! purpose-built tables — the part the client-level `group_round_trip` tests
//! reach only indirectly.
//!
//! The commit-log key is the interesting one: callers pass the RAW group id, and
//! the Postgres impl stores it as the table's primary key with no bincode
//! round-trip (the indirection the old generic byte-KV forced). This asserts that
//! directly, plus that no generic `openmls_key_value` backup table exists at all.
//!
//! Pure Postgres (no XMTP node); skipped unless `XMTP_ASYNCDB_PG_URL` is set.

use sqlx::Row;
use xmtp_db::XmtpMlsStorageProvider;
use xmtp_mls_pg_tests::{bare_key_store, pg_url_or_skip};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn commit_log_key_lands_in_typed_table_keyed_by_raw_group_id() {
    let url = pg_url_or_skip!();
    xmtp_mls_pg_tests::init_logging();

    let (store, db, _schema) = bare_key_store(&url, "commitlog")
        .await
        .expect("bare key store");

    // `write_commit_log_key` stores `bincode(secret_bytes)` as the value; mirror
    // that. The group id is passed RAW.
    let group_id: Vec<u8> = (0u8..16).collect();
    let private_key_bytes = vec![9u8; 32];
    let value = bincode::serialize(&private_key_bytes).expect("encode value");

    store
        .set_commit_log_signer_key(&group_id, &value)
        .await
        .expect("set commit-log key");

    // 1. Round-trips through the typed accessor.
    let got: Option<Vec<u8>> = store
        .commit_log_signer_key::<Vec<u8>>(&group_id)
        .await
        .expect("get commit-log key");
    assert_eq!(
        got,
        Some(private_key_bytes),
        "commit-log key must round-trip through the typed table unchanged"
    );

    // 2. The typed table's PK is the RAW group id (no bincode wrapping), and the
    //    value column holds the caller's bytes verbatim.
    let mut c = db.conn().await.expect("conn");
    let row = sqlx::query("SELECT group_id, private_key FROM commit_log_signer_keys")
        .fetch_one(&mut *c)
        .await
        .expect("one commit_log_signer_keys row");
    assert_eq!(
        row.get::<Vec<u8>, _>("group_id"),
        group_id,
        "the typed PK is the raw group id passed by the caller"
    );
    assert_eq!(
        row.get::<Vec<u8>, _>("private_key"),
        value,
        "the value column holds the caller's bytes verbatim"
    );

    // 3. There is no generic backup table at all — data can't be stranded.
    let kv_present: bool =
        sqlx::query_scalar("SELECT to_regclass('openmls_key_value') IS NOT NULL")
            .fetch_one(&mut *c)
            .await
            .expect("to_regclass openmls_key_value");
    assert!(
        !kv_present,
        "the generic openmls_key_value backup table must not exist"
    );

    eprintln!("COMMIT-LOG TYPED ACCESSOR OK: raw group id, typed table, no backup KV");
}
