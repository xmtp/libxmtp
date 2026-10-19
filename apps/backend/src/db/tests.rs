use crate::{
    config::{Config, ValidatedConfig},
    db::Store,
    test_support::{TestDatabase, TestServer, config_for},
};
use sqlx::Connection;
use xmtp_common::time::{Duration, timeout};
use xmtp_mls_validation::test_utils::inline_welcome_envelope;

const PRIMARY_READ_STARTUP_TIMEOUT: Duration = Duration::from_secs(5);

#[xmtp_common::test(unwrap_try = true)]
async fn primary_read_pool_does_not_wait_for_recovery() {
    let mut database = TestDatabase::new()?;
    let mut config = config_for(database.url());
    config.database.replica_url = Some(database.url().to_owned());
    let config = config.validate()?;
    let store = timeout(PRIMARY_READ_STARTUP_TIMEOUT, Store::connect(&config)).await??;
    assert!(
        !sqlx::query_scalar::<_, bool>(sqlx::AssertSqlSafe("SELECT pg_is_in_recovery()"))
            .fetch_one(&store.read)
            .await?
    );
    assert_eq!(
        sqlx::query_scalar!("SELECT closed_sequence_id FROM allocation_boundary WHERE singleton")
            .fetch_one(&store.read)
            .await?,
        0
    );
    store.read.close().await;
    store.primary.close().await;
    database.remove()?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn cancelled_begin_returns_a_clean_connection() {
    let Some(metrics) = crate::test_support::metrics::isolated(
        "db::tests::cancelled_begin_returns_a_clean_connection",
    ) else {
        return;
    };
    let database = TestDatabase::new()?;
    let mut config = config_for(database.url());
    config.database.max_connections = 1;
    let config = config.validate()?;
    let store = Store::connect(&config).await?;
    let mut connection = store.primary.acquire().await?;
    let pid = sqlx::query_scalar::<_, i32>("SELECT pg_backend_pid()")
        .fetch_one(&mut *connection)
        .await?;
    {
        let mut begin = std::pin::pin!(
            connection.begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
        );
        assert!(futures::poll!(&mut begin).is_pending());
    }
    drop(connection);

    let mut connection = store.primary.acquire().await?;
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT pg_backend_pid()")
            .fetch_one(&mut *connection)
            .await?,
        pid,
        "an open transaction must be rolled back without replacing the connection"
    );
    let mut tx = connection
        .begin_with("BEGIN ISOLATION LEVEL READ COMMITTED")
        .await?;
    let sequence = sqlx::query_scalar::<_, i64>("SELECT nextval('envelope_sequence')")
        .fetch_one(&mut *tx)
        .await?;
    assert!(sequence > 0);
    tx.rollback().await?;
    drop(connection);
    store.primary.close().await;
    assert_eq!(
        crate::test_support::metrics::value(
            &metrics,
            "xmtp_db_released_open_transactions_total",
            &[]
        ),
        1.0
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn aborted_transaction_is_replaced_before_pool_reuse() {
    let database = TestDatabase::new()?;
    let mut config = config_for(database.url());
    config.database.max_connections = 1;
    let config = config.validate()?;
    let store = Store::connect(&config).await?;
    let mut connection = store.primary.acquire().await?;
    let pid = sqlx::query_scalar::<_, i32>("SELECT pg_backend_pid()")
        .fetch_one(&mut *connection)
        .await?;
    sqlx::query("BEGIN").execute(&mut *connection).await?;
    let error = sqlx::query("SELECT 1 / 0")
        .execute(&mut *connection)
        .await
        .unwrap_err();
    assert!(
        matches!(error, sqlx::Error::Database(error) if error.code().as_deref() == Some("22012"))
    );
    drop(connection);

    let mut connection = store.primary.acquire().await?;
    let replacement_pid = sqlx::query_scalar::<_, i32>("SELECT pg_backend_pid()")
        .fetch_one(&mut *connection)
        .await?;
    assert_ne!(replacement_pid, pid);
    let mut tx = connection
        .begin_with("BEGIN ISOLATION LEVEL READ COMMITTED")
        .await?;
    let sequence = sqlx::query_scalar::<_, i64>("SELECT nextval('envelope_sequence')")
        .fetch_one(&mut *tx)
        .await?;
    assert!(sequence > 0);
    tx.rollback().await?;
    drop(connection);
    store.primary.close().await;
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: OPS-020
async fn startup_rejects_retention_that_overflows_the_database_clock() {
    let database = TestDatabase::new()?;
    let mut config: Config = toml::from_str(&format!(
        "[database]\nurl = {:?}\n[server]\nidentifier = {:?}",
        database.url(),
        crate::test_support::DEFAULT_TEST_IDENTIFIER
    ))?;
    config.retention.welcome_seconds = (i64::MAX / xmtp_common::NS_IN_SEC) as u64;
    let error = crate::server::initialize(config.validate()?)
        .await
        .err()
        .expect("startup expiry must fit");
    let error = error.downcast_ref::<crate::config::ConfigError>()?;
    assert!(
        matches!(error, crate::config::ConfigError::Invalid { .. }),
        "{error}"
    );
    assert_eq!(error.field(), Some("retention.welcome_seconds"), "{error}");
}

/// A push expiry that passes the load check but overflows at the store's
/// clock stops startup under its key.
#[xmtp_common::test(unwrap_try = true)]
// verifies: CONF-091
async fn startup_rejects_push_expiry_that_overflows_the_database_clock() {
    let database = TestDatabase::new()?;
    let config = ValidatedConfig::load_str(&format!(
        "[database]\nurl = {:?}\n[server]\nidentifier = {:?}\n[push]\nrecipient_ttl_seconds = 9223372036\n",
        database.url(),
        crate::test_support::DEFAULT_TEST_IDENTIFIER
    ))?;
    let error = crate::server::initialize(config)
        .await
        .err()
        .expect("startup push expiry must fit");
    let error = error.downcast_ref::<crate::config::ConfigError>()?;
    assert!(
        matches!(error, crate::config::ConfigError::Invalid { .. }),
        "{error}"
    );
    assert_eq!(error.field(), Some("push.recipient_ttl_seconds"), "{error}");
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: OPS-005
async fn concurrent_initializers_apply_one_migration_on_an_empty_database() {
    let mut database = TestDatabase::new()?;
    let config = config_for(database.url()).validate()?;
    let (left, right) = tokio::join!(Store::connect(&config), Store::connect(&config));
    let left = left?;
    let right = right?;
    let migrations = sqlx::query_scalar!("SELECT count(*) FROM _sqlx_migrations WHERE success")
        .fetch_one(&left.primary)
        .await?;
    // Each migration is recorded once, not once per initializer.
    let defined = sqlx::migrate!().iter().count();
    assert_eq!(migrations, Some(i64::try_from(defined)?));
    assert_eq!(
        sqlx::query_scalar!("SELECT closed_sequence_id FROM allocation_boundary WHERE singleton")
            .fetch_one(&right.primary)
            .await?,
        0
    );
    left.primary.close().await;
    right.primary.close().await;
    database.remove()?;
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: API-286, API-287
async fn storage_constraints_reject_invalid_rows() {
    let server = TestServer::new(|_| {}).await?;
    server
        .publish(vec![inline_welcome_envelope([9; 32])])
        .await?;

    for (statement, expected_code) in [
        (
            "INSERT INTO envelopes SELECT 0, topic, server_ns, expiry_ns, message_hash, is_commit_or_proposal, payload, push_eligible, sender_hmac FROM envelopes LIMIT 1",
            "23514",
        ),
        (
            "INSERT INTO envelopes SELECT 99, topic, server_ns, expiry_ns, message_hash, is_commit_or_proposal, payload, push_eligible, sender_hmac FROM envelopes LIMIT 1",
            "23505",
        ),
        (
            "INSERT INTO envelopes SELECT sequence_id, topic, server_ns, expiry_ns, message_hash, is_commit_or_proposal, payload, push_eligible, sender_hmac FROM envelopes LIMIT 1",
            "23505",
        ),
        (
            "INSERT INTO envelopes SELECT 99, topic, server_ns, expiry_ns, 'x'::bytea, is_commit_or_proposal, payload, push_eligible, sender_hmac FROM envelopes LIMIT 1",
            "23514",
        ),
    ] {
        let error = sqlx::query(statement)
            .execute(&server.backend.store.primary)
            .await
            .expect_err("invalid storage row must be rejected");
        let code = match error {
            sqlx::Error::Database(error) => error.code().map(|code| code.to_string()),
            error => panic!("expected database constraint error, got {error}"),
        };
        assert_eq!(code.as_deref(), Some(expected_code));
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM envelopes")
            .fetch_one(&server.backend.store.primary)
            .await?,
        1
    );
    server.stop().await?;
}

/// Insert one association row directly, bypassing admission.
async fn insert_association(
    store: &Store,
    identifier: &str,
    kind: i16,
    inbox_id: [u8; 32],
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO identifier_association
            (identifier, identifier_kind, inbox_id, association_sequence_id)
         VALUES ($1, $2, $3, 1)",
    )
    .bind(identifier)
    .bind(kind)
    .bind(inbox_id.to_vec())
    .execute(&store.primary)
    .await
    .map(|_| ())
}

/// The handle index admits one inbox per handle, as a backstop for the
/// admission check. Other kinds are not affected: one Ethereum address may
/// be associated with two inboxes, and an Ethereum row with a handle's text
/// does not take the handle.
#[xmtp_common::test(unwrap_try = true)]
async fn handle_index_allows_one_inbox_per_handle() {
    let mut database = TestDatabase::new()?;
    let config = config_for(database.url()).validate()?;
    let store = Store::connect(&config).await?;
    let address = "0x1111111111111111111111111111111111111111";

    insert_association(&store, "nick", 3, [1; 32]).await?;
    let error = insert_association(&store, "nick", 3, [2; 32])
        .await
        .unwrap_err();
    assert!(
        matches!(&error, sqlx::Error::Database(error) if error.code().as_deref() == Some("23505")),
        "{error:?}"
    );
    insert_association(&store, address, 1, [1; 32]).await?;
    insert_association(&store, address, 1, [2; 32]).await?;
    insert_association(&store, "nick", 1, [2; 32]).await?;

    store.primary.close().await;
    database.remove()?;
}
