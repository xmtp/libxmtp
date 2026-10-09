use super::*;
use crate::test_support::{TestDatabase, TestResult, TestServer};
use sqlx::{Connection, PgConnection};

const MISSING_BOUNDARY_BUDGET: Duration = Duration::from_millis(100);
const BOUNDARY_RETRY_BUDGET: Duration = Duration::from_secs(5);
const BOUNDARY_RETRY_OBSERVATION: Duration = Duration::from_millis(50);
const PERMANENT_BOUNDARY_ERROR_DEADLINE: Duration = Duration::from_secs(1);
const BOUNDARY_TEST_DEADLINE: Duration = Duration::from_secs(5);
const RECOVERED_BOUNDARY: i64 = 7;

#[xmtp_common::test(unwrap_try = true)]
async fn missing_startup_boundary_expires_and_releases_its_snapshot() {
    let mut database = TestDatabase::new()?;
    let mut connection = PgConnection::connect(database.url()).await?;
    let result = xmtp_common::time::timeout(
        BOUNDARY_TEST_DEADLINE,
        startup_boundary(&mut connection, MISSING_BOUNDARY_BUDGET),
    )
    .await?;
    match result {
        Err(Error::StartupBoundaryTimeout { timeout_ms }) => {
            assert_eq!(timeout_ms, MISSING_BOUNDARY_BUDGET.as_millis() as u64);
        }
        result => panic!("expected missing-schema startup timeout, got {result:?}"),
    }
    // Failed missing-table snapshots must not poison the caller's connection.
    create_allocation_boundary(&mut connection, Some(RECOVERED_BOUNDARY)).await?;
    let visible =
        sqlx::query_scalar!("SELECT closed_sequence_id FROM allocation_boundary WHERE singleton")
            .fetch_one(&mut connection)
            .await?;
    assert_eq!(visible, RECOVERED_BOUNDARY);
    connection.close().await?;
    database.remove()?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn missing_startup_boundary_row_fails_without_retrying() {
    let mut database = TestDatabase::new()?;
    let mut connection = PgConnection::connect(database.url()).await?;
    create_allocation_boundary(&mut connection, None).await?;
    let result = xmtp_common::time::timeout(
        PERMANENT_BOUNDARY_ERROR_DEADLINE,
        startup_boundary(&mut connection, BOUNDARY_RETRY_BUDGET),
    )
    .await?;
    xmtp_common::assert_err!(result, Error::Invariant(_));
    connection.close().await?;
    database.remove()?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn invalid_startup_boundary_column_fails_without_retrying() {
    let mut database = TestDatabase::new()?;
    let mut connection = PgConnection::connect(database.url()).await?;
    sqlx::raw_sql(sqlx::AssertSqlSafe(
        "CREATE TABLE allocation_boundary (singleton boolean PRIMARY KEY)",
    ))
    .execute(&mut connection)
    .await?;
    let result = xmtp_common::time::timeout(
        PERMANENT_BOUNDARY_ERROR_DEADLINE,
        startup_boundary(&mut connection, BOUNDARY_RETRY_BUDGET),
    )
    .await?;
    match result {
        Err(Error::Database(sqlx::Error::Database(error))) => {
            assert_eq!(error.code().as_deref(), Some("42703"));
        }
        result => panic!("expected permanent undefined-column error, got {result:?}"),
    }
    connection.close().await?;
    database.remove()?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn startup_boundary_retries_with_a_fresh_snapshot_until_schema_arrives() {
    let mut database = TestDatabase::new()?;
    let mut connection = PgConnection::connect(database.url()).await?;
    let mut writer = PgConnection::connect(database.url()).await?;
    let visible = {
        let mut startup = std::pin::pin!(startup_boundary(&mut connection, BOUNDARY_RETRY_BUDGET,));
        assert!(
            xmtp_common::time::timeout(BOUNDARY_RETRY_OBSERVATION, &mut startup)
                .await
                .is_err(),
            "startup must wait while its boundary table is missing",
        );
        create_allocation_boundary(&mut writer, Some(RECOVERED_BOUNDARY)).await?;
        xmtp_common::time::timeout(BOUNDARY_TEST_DEADLINE, startup).await??
    };
    assert_eq!(visible, RECOVERED_BOUNDARY);
    writer.close().await?;
    connection.close().await?;
    database.remove()?;
}

async fn create_allocation_boundary(
    connection: &mut PgConnection,
    boundary: Option<i64>,
) -> TestResult {
    let mut transaction = connection.begin().await?;
    sqlx::raw_sql(sqlx::AssertSqlSafe(
        "CREATE TABLE allocation_boundary (singleton boolean PRIMARY KEY, closed_sequence_id bigint NOT NULL)",
    ))
    .execute(&mut *transaction)
    .await?;
    if let Some(boundary) = boundary {
        sqlx::query(sqlx::AssertSqlSafe(
            "INSERT INTO allocation_boundary (singleton, closed_sequence_id) VALUES (true, $1)",
        ))
        .bind(boundary)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(())
}

#[xmtp_common::test(unwrap_try = true)]
async fn signals_during_cooldown_share_one_boundary_attempt() {
    let server = TestServer::new(|_| {}).await?;
    server.backend.streams.as_ref().unwrap().stop();
    let pool = &server.backend.store.primary;
    sqlx::query!("SELECT nextval('envelope_sequence')")
        .fetch_one(pool)
        .await?;
    let signal = Arc::new(Notify::new());
    signal.notify_one();
    let mut maintenance = Box::pin(maintain(
        pool.clone(),
        signal.clone(),
        Duration::from_millis(250),
        10,
        Instant::now(),
    ));
    // Poll through the first notification into the cooldown before sending more.
    assert!(futures::poll!(&mut maintenance).is_pending());
    let worker = Worker(tokio::spawn(maintenance));
    for _ in 0..1_000 {
        signal.notify_one();
    }
    xmtp_common::wait_for_eq(|| boundary(pool), 1).await?;
    sqlx::query!("SELECT nextval('envelope_sequence')")
        .fetch_one(pool)
        .await?;
    let extra = xmtp_common::time::timeout(
        Duration::from_millis(400),
        xmtp_common::wait_for_eq(|| boundary(pool), 2),
    )
    .await;
    assert!(
        extra.is_err(),
        "cooldown signals scheduled a second attempt"
    );
    signal.notify_one();
    xmtp_common::wait_for_eq(|| boundary(pool), 2).await?;
    drop(worker);
    server.stop().await?;
}

async fn boundary(pool: &PgPool) -> i64 {
    sqlx::query_scalar!("SELECT closed_sequence_id FROM allocation_boundary WHERE singleton")
        .fetch_one(pool)
        .await
        .unwrap()
}
