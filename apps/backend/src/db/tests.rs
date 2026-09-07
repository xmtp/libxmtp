use crate::{
    config::Config,
    db::Store,
    test_support::{TestDatabase, TestServer},
};
use xmtp_mls_validation::test_utils::inline_welcome_envelope;

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
