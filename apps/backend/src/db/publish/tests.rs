use crate::test_support as support;

use crate::api;
use alloy_primitives::Bytes;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use support::TestServer;
use tokio::sync::Notify;
use tonic::Code;
use xmtp_id::{
    associations::AccountId,
    scw_verifier::{SmartContractSignatureVerifier, ValidationResponse, VerifierError},
};
use xmtp_mls_validation::test_utils::{identity_envelope, scw_create_inbox_update};

#[xmtp_common::test(unwrap_try = true)]
async fn new_envelopes_use_transaction_start_time_even_after_a_wait() {
    let server = TestServer::new(|_| {}).await?;
    let mut pending = Vec::new();
    for index in 0..2 {
        let parsed = xmtp_mls_validation::parse_envelope(
            xmtp_mls_validation::test_utils::inline_welcome_envelope([index as u8; 32]),
        )?;
        pending.push(super::PendingEnvelope {
            topic: parsed.topic.to_vec(),
            message_hash: parsed.canonical.hash,
            payload: parsed.canonical.bytes,
            is_commit_or_proposal: false,
            identity: None,
            index,
            duplicate: None,
            validation: Ok(None),
            retention_ns: Some(xmtp_common::NS_IN_SEC),
        });
    }
    let mut tx = server.backend.store.primary.begin().await?;
    let started: i64 =
        sqlx::query_scalar("SELECT (extract(epoch FROM CURRENT_TIMESTAMP) * $1::bigint)::bigint")
            .bind(xmtp_common::NS_IN_SEC)
            .fetch_one(&mut *tx)
            .await?;
    super::lock(&mut tx, &pending).await?;
    // Let the database clock advance without changing the transaction timestamp.
    sqlx::query("SELECT pg_sleep(0.01)")
        .execute(&mut *tx)
        .await?;
    let rows = super::insert(&mut tx, &pending.iter().collect::<Vec<_>>()).await?;
    assert_eq!(rows.len(), pending.len());
    for row in rows {
        assert_eq!(row.server_ns, started);
        assert_eq!(row.expiry_ns, Some(started + xmtp_common::NS_IN_SEC));
    }
    tx.commit().await?;
    server.stop().await?;
}

struct PausedVerifier {
    calls: AtomicUsize,
    entered: Arc<Notify>,
    resume: Arc<Notify>,
    first_valid: bool,
}

#[xmtp_common::async_trait]
impl SmartContractSignatureVerifier for PausedVerifier {
    async fn is_valid_signature(
        &self,
        _: AccountId,
        _: [u8; 32],
        _: Bytes,
        block_number: Option<u64>,
    ) -> Result<ValidationResponse, VerifierError> {
        let first = self.calls.fetch_add(1, Ordering::SeqCst) == 0;
        if first {
            self.entered.notify_one();
            self.resume.notified().await;
        }
        Ok(ValidationResponse {
            is_valid: !first || self.first_valid,
            block_number,
            error: None,
        })
    }
}

async fn paused_server(
    first_valid: bool,
) -> support::TestResult<(TestServer, Arc<Notify>, Arc<Notify>)> {
    let entered = Arc::new(Notify::new());
    let resume = Arc::new(Notify::new());
    let verifier = PausedVerifier {
        calls: AtomicUsize::new(0),
        entered: entered.clone(),
        resume: resume.clone(),
        first_valid,
    };
    Ok((
        TestServer::with_verifier(|_| {}, verifier).await?,
        entered,
        resume,
    ))
}

#[xmtp_common::test(unwrap_try = true)]
async fn committed_duplicate_wins_over_a_concurrent_validation_failure() {
    let (server, entered, resume) = paused_server(false).await?;
    let envelope = identity_envelope(scw_create_inbox_update());
    let request = api::PublishRequest {
        envelopes: vec![envelope.clone()],
    };
    let mut client = server.publisher();
    let first = tokio::spawn(async move { client.publish(request).await });
    entered.notified().await;
    let committed = server.publish(vec![envelope]).await?;
    resume.notify_one();
    assert_eq!(first.await??.into_inner().envelope_metas, committed);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn changed_history_aborts_all_new_rows_after_verification() {
    let (server, entered, resume) = paused_server(true).await?;
    let original = scw_create_inbox_update();
    let request = api::PublishRequest {
        envelopes: vec![
            identity_envelope(original.clone()),
            xmtp_mls_validation::test_utils::inline_welcome_envelope([9; 32]),
        ],
    };
    let mut client = server.publisher();
    let first = tokio::spawn(async move { client.publish(request).await });
    entered.notified().await;
    let mut replacement = original;
    replacement.client_timestamp_ns += 1;
    server.publish(vec![identity_envelope(replacement)]).await?;
    resume.notify_one();
    assert_eq!(first.await?.unwrap_err().code(), Code::Aborted);
    let count = sqlx::query_scalar!("SELECT count(*) FROM envelopes")
        .fetch_one(&server.backend.store.primary)
        .await?;
    assert_eq!(count, Some(1));
    server.stop().await?;
}

fn publish_task(
    server: &TestServer,
    envelope: api::ClientEnvelope,
) -> tokio::task::JoinHandle<Result<tonic::Response<api::PublishResponse>, tonic::Status>> {
    let mut client = server.publisher();
    tokio::spawn(async move {
        client
            .publish(api::PublishRequest {
                envelopes: vec![envelope],
            })
            .await
    })
}

async fn wait_for_lock(pool: &sqlx::PgPool, lock_type: &'static str) -> support::TestResult {
    xmtp_common::wait_for_eq(
        || async {
            sqlx::query_scalar!(
                r#"SELECT EXISTS (SELECT 1 FROM pg_locks
                WHERE database = (SELECT oid FROM pg_database WHERE datname = current_database())
                  AND locktype = $1 AND NOT granted) AS "waiting!""#,
                lock_type
            )
            .fetch_one(pool)
            .await
            .unwrap()
        },
        true,
    )
    .await?;
    Ok(())
}

async fn ordered_writers(
    first: api::ClientEnvelope,
    second: api::ClientEnvelope,
) -> support::TestResult {
    let server = TestServer::new(|_| {}).await?;
    let pool = &server.backend.store.primary;
    let mut blocker = pool.begin().await?;
    sqlx::query!("LOCK TABLE envelopes IN SHARE MODE")
        .execute(&mut *blocker)
        .await?;
    let first = publish_task(&server, first);
    wait_for_lock(pool, "relation").await?;
    let second = publish_task(&server, second);
    wait_for_lock(pool, "advisory").await?;
    // Only the first writer can allocate before its transaction finishes.
    assert_eq!(
        sqlx::query_scalar!("SELECT last_value FROM envelope_sequence")
            .fetch_one(pool)
            .await?,
        1
    );
    assert_eq!(
        sqlx::query_scalar!("SELECT count(*) FROM envelopes")
            .fetch_one(pool)
            .await?,
        Some(0)
    );
    blocker.commit().await?;
    let first = first.await??.into_inner().envelope_metas.remove(0);
    let second = second.await??.into_inner().envelope_metas.remove(0);
    assert_eq!(first.cursor.unwrap().sequence_id, 1);
    assert_eq!(second.cursor.unwrap().sequence_id, 2);
    server.stop().await?;
    Ok(())
}

#[xmtp_common::test(unwrap_try = true)]
async fn distinct_same_topic_writers_allocate_in_commit_order() {
    use xmtp_mls_validation::test_utils::{inline_welcome_envelope, welcome_pointer_envelope};
    ordered_writers(
        inline_welcome_envelope([10; 32]),
        welcome_pointer_envelope([10; 32]),
    )
    .await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn identity_writers_on_distinct_inboxes_allocate_in_commit_order() {
    use xmtp_mls_validation::test_utils::identity_history_with_passkey;
    let first = identity_history_with_passkey().await;
    let second = identity_history_with_passkey().await;
    assert_ne!(first.inbox_id, second.inbox_id);
    ordered_writers(
        identity_envelope(first.history[0].clone()),
        identity_envelope(second.history[0].clone()),
    )
    .await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn timed_out_publish_releases_locks_and_discards_allocated_rows() {
    let server = TestServer::new(|config| {
        config.database.max_statement_timeout_ms = 500;
        config.publishing.max_publish_duration_ms = 1_000;
    })
    .await?;
    let pool = &server.backend.store.primary;
    let mut blocker = pool.begin().await?;
    sqlx::query!("LOCK TABLE envelopes IN SHARE MODE")
        .execute(&mut *blocker)
        .await?;
    let envelope = xmtp_mls_validation::test_utils::inline_welcome_envelope([11; 32]);
    let publish = publish_task(&server, envelope.clone());
    wait_for_lock(pool, "relation").await?;
    let result =
        xmtp_common::time::timeout(xmtp_common::time::Duration::from_secs(2), publish).await??;
    assert_eq!(result.unwrap_err().code(), Code::DeadlineExceeded);
    blocker.commit().await?;
    let retried = server.publish(vec![envelope]).await?;
    assert_eq!(retried[0].cursor.as_ref().unwrap().sequence_id, 2);
    assert_eq!(
        sqlx::query_scalar!("SELECT count(*) FROM envelopes")
            .fetch_one(pool)
            .await?,
        Some(1)
    );
    assert_eq!(
        sqlx::query_scalar!("SELECT count(*) FROM topic_watermark")
            .fetch_one(pool)
            .await?,
        Some(1)
    );
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn canceled_request_releases_its_transaction_before_a_retry() {
    let server = TestServer::new(|_| {}).await?;
    let pool = &server.backend.store.primary;
    let mut blocker = pool.begin().await?;
    sqlx::query!("LOCK TABLE envelopes IN SHARE MODE")
        .execute(&mut *blocker)
        .await?;
    let envelope = xmtp_mls_validation::test_utils::inline_welcome_envelope([12; 32]);
    let publish = publish_task(&server, envelope.clone());
    wait_for_lock(pool, "relation").await?;
    publish.abort();
    assert!(publish.await.unwrap_err().is_cancelled());
    xmtp_common::wait_for_eq(
        || async {
            sqlx::query_scalar!(
                "SELECT count(*) FROM pg_locks WHERE locktype = 'advisory'
                AND database = (SELECT oid FROM pg_database WHERE datname = current_database())"
            )
            .fetch_one(pool)
            .await
            .unwrap()
        },
        Some(0),
    )
    .await?;
    blocker.commit().await?;
    let result = xmtp_common::time::timeout(
        xmtp_common::time::Duration::from_secs(2),
        server.publish(vec![envelope]),
    )
    .await??;
    assert_eq!(result[0].cursor.as_ref().unwrap().sequence_id, 2);
    assert_eq!(
        sqlx::query_scalar!("SELECT count(*) FROM envelopes")
            .fetch_one(pool)
            .await?,
        Some(1)
    );
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn cumulative_publish_deadline_releases_database_locks_before_statement_timeout() {
    use xmtp_common::time::{Duration, timeout};
    const STATEMENT_MS: u64 = 4_000;
    const PUBLISH_MS: u64 = 6_000;
    const FIRST_WAIT_SECONDS: f64 = 3.0;
    const CLEANUP_ALLOWANCE_MS: u64 = 500;

    let server = TestServer::new(|config| {
        config.database.max_statement_timeout_ms = STATEMENT_MS;
        config.publishing.max_publish_duration_ms = PUBLISH_MS;
    })
    .await?;
    let pool = &server.backend.store.primary;
    let envelope = xmtp_mls_validation::test_utils::inline_welcome_envelope([13; 32]);
    let parsed = xmtp_mls_validation::parse_envelope(envelope.clone())?;
    let hash = xmtp_common::sha256_array(&parsed.topic);
    let key = i64::from_be_bytes(hash[..8].try_into()?);
    let mut topic_blocker = pool.begin().await?;
    sqlx::query!("SELECT pg_advisory_xact_lock($1::bigint)", key)
        .execute(&mut *topic_blocker)
        .await?;
    let mut table_blocker = pool.begin().await?;
    sqlx::query!("LOCK TABLE envelopes IN SHARE MODE")
        .execute(&mut *table_blocker)
        .await?;

    let publish = publish_task(&server, envelope.clone());
    // Let the first statement consume part of the transaction budget, not its own limit.
    xmtp_common::wait_for_eq(
        || async {
            sqlx::query_scalar!(
                r#"SELECT EXISTS (SELECT 1 FROM pg_stat_activity
                WHERE datname = current_database() AND wait_event = 'advisory'
                  AND extract(epoch FROM clock_timestamp() - xact_start)::float8 >= $1)
                AS "elapsed!""#,
                FIRST_WAIT_SECONDS
            )
            .fetch_one(pool)
            .await
            .unwrap()
        },
        true,
    )
    .await?;
    topic_blocker.commit().await?;
    wait_for_lock(pool, "relation").await?;

    let response = timeout(Duration::from_millis(PUBLISH_MS), publish).await??;
    assert_eq!(response.unwrap_err().code(), Code::DeadlineExceeded);
    // The final INSERT remains blocked. A Rust timeout alone leaves its locks held
    // until the second statement's later timeout. The database deadline must end it first.
    timeout(
        Duration::from_millis(CLEANUP_ALLOWANCE_MS),
        xmtp_common::wait_for_eq(
            || async {
                sqlx::query_scalar!(
                    "SELECT count(*) FROM pg_locks WHERE locktype = 'advisory'
                AND database = (SELECT oid FROM pg_database WHERE datname = current_database())"
                )
                .fetch_one(pool)
                .await
                .unwrap()
            },
            Some(0),
        ),
    )
    .await??;
    assert_eq!(
        sqlx::query_scalar!("SELECT count(*) FROM envelopes")
            .fetch_one(pool)
            .await?,
        Some(0)
    );
    table_blocker.commit().await?;
    let retried = server.publish(vec![envelope]).await?;
    assert_eq!(retried[0].cursor.as_ref().unwrap().sequence_id, 2);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn projection_failure_after_writes_rolls_back_the_publish_transaction() {
    let server = TestServer::new(|_| {}).await?;
    let pool = &server.backend.store.primary;
    sqlx::raw_sql("CREATE SEQUENCE test_projection_seen; CREATE FUNCTION test_reject_projection() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF (SELECT count(*) FROM envelopes) = 2 AND (SELECT count(*) FROM topic_watermark) = 2 AND (SELECT count(*) FROM identifier_association) = 1 THEN PERFORM nextval('test_projection_seen'); END IF; RAISE EXCEPTION USING ERRCODE = '23514', MESSAGE = 'injected projection failure'; END $$; CREATE TRIGGER test_projection_failure AFTER INSERT ON identifier_association FOR EACH ROW EXECUTE FUNCTION test_reject_projection();")
        .execute(pool)
        .await?;
    let fixture = xmtp_mls_validation::test_utils::identity_history_with_passkey().await;
    let result = server
        .publisher()
        .publish(api::PublishRequest {
            envelopes: vec![
                identity_envelope(fixture.history[0].clone()),
                xmtp_mls_validation::test_utils::inline_welcome_envelope([61; 32]),
            ],
        })
        .await;
    assert_eq!(result.unwrap_err().code(), Code::Internal);
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT is_called FROM test_projection_seen")
            .fetch_one(pool)
            .await?
    );
    let envelopes: i64 = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM envelopes")
        .fetch_one(pool)
        .await?;
    let watermarks: i64 = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM topic_watermark")
        .fetch_one(pool)
        .await?;
    let projections: i64 =
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM identifier_association")
            .fetch_one(pool)
            .await?;
    assert_eq!(envelopes, 0);
    assert_eq!(watermarks, 0);
    assert_eq!(projections, 0);
    server.stop().await?;
}
