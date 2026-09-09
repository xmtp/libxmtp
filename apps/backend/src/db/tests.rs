use crate::{
    config::Config,
    db::Store,
    test_support::{TestDatabase, TestServer},
};
use sqlx::Connection;
use xmtp_mls_validation::test_utils::inline_welcome_envelope;

#[xmtp_common::test(unwrap_try = true)]
async fn cancelled_begin_returns_a_clean_connection() {
    let Some(metrics) = crate::test_support::metrics::isolated(
        "db::tests::cancelled_begin_returns_a_clean_connection",
    ) else {
        return;
    };
    let database = TestDatabase::new()?;
    let mut config: Config = toml::from_str(&format!("[database]\nurl = {:?}", database.url()))?;
    config.database.max_connections = 1;
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
    let mut config: Config = toml::from_str(&format!("[database]\nurl = {:?}", database.url()))?;
    config.database.max_connections = 1;
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
async fn startup_rejects_retention_that_overflows_the_database_clock() {
    let database = TestDatabase::new()?;
    let mut config: Config = toml::from_str(&format!("[database]\nurl = {:?}", database.url()))?;
    config.retention.welcome_seconds = (i64::MAX / xmtp_common::NS_IN_SEC) as u64;
    config.validate()?;
    let error = crate::server::initialize(config)
        .await
        .err()
        .expect("startup expiry must fit");
    assert!(matches!(
        error.downcast_ref::<crate::config::ConfigError>(),
        Some(crate::config::ConfigError::Invalid {
            field: "retention.welcome_seconds",
            ..
        })
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn concurrent_initializers_apply_one_migration_on_an_empty_database() {
    let mut database = TestDatabase::new()?;
    let config: Config = toml::from_str(&format!("[database]\nurl = {:?}", database.url()))?;
    let (left, right) = tokio::join!(Store::connect(&config), Store::connect(&config));
    let left = left?;
    let right = right?;
    let migrations = sqlx::query_scalar!("SELECT count(*) FROM _sqlx_migrations WHERE success")
        .fetch_one(&left.primary)
        .await?;
    assert_eq!(migrations, Some(1));
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
async fn storage_constraints_reject_invalid_rows() {
    let server = TestServer::new(|_| {}).await?;
    server
        .publish(vec![inline_welcome_envelope([9; 32])])
        .await?;

    for (statement, expected_code) in [
        (
            "INSERT INTO envelopes SELECT 0, topic, server_ns, expiry_ns, message_hash, is_commit_or_proposal, payload FROM envelopes LIMIT 1",
            "23514",
        ),
        (
            "INSERT INTO envelopes SELECT 99, topic, server_ns, expiry_ns, message_hash, is_commit_or_proposal, payload FROM envelopes LIMIT 1",
            "23505",
        ),
        (
            "INSERT INTO envelopes SELECT sequence_id, topic, server_ns, expiry_ns, message_hash, is_commit_or_proposal, payload FROM envelopes LIMIT 1",
            "23505",
        ),
        (
            "INSERT INTO envelopes SELECT 99, topic, server_ns, expiry_ns, 'x'::bytea, is_commit_or_proposal, payload FROM envelopes LIMIT 1",
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
