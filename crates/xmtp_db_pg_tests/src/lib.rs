//! Test harness for xmtp_db's async storage track.
//!
//! These tests run against a real PostgreSQL because that is the entire point of
//! the async track: a query stops being an in-process call and becomes a network
//! round-trip. The failure modes worth covering — contention, isolation,
//! rollback, concurrency, retry classification — do not exist against SQLite and
//! cannot be reproduced with a mock.
//!
//! Point `XMTP_ASYNCDB_PG_URL` at a scratch database, e.g.
//! `postgres://xmtp:xmtp@127.0.0.1:55432/xmtp_asyncdb`.

use sqlx::Executor;
use sqlx::postgres::PgPoolOptions;
use xmtp_db::pg::PgDb;

/// The committed Postgres schema, compiled in rather than read at runtime so the
/// tests can never silently run against a stale or hand-edited database.
pub const SCHEMA_SQL: &str = include_str!("../../xmtp_db/migrations_pg/0000_init/up.sql");

pub fn database_url() -> String {
    std::env::var("XMTP_ASYNCDB_PG_URL").expect(
        "XMTP_ASYNCDB_PG_URL must point at a scratch Postgres, e.g. \
         postgres://xmtp:xmtp@127.0.0.1:55432/xmtp_asyncdb",
    )
}

/// A `PgDb` over a private, freshly-created Postgres schema.
///
/// Each test owns a namespace so the suite runs in parallel without tests seeing
/// each other's rows. `search_path` is set via `after_connect` rather than once
/// up front: the pool hands out a different connection per query, so setting it
/// on a single connection would silently apply to only that one.
async fn scoped_db(schema: &str, setup_sql: Option<&str>) -> PgDb {
    let url = database_url();

    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("connect to Postgres");
    admin
        .execute(format!("DROP SCHEMA IF EXISTS {schema} CASCADE").as_str())
        .await
        .expect("drop schema");
    admin
        .execute(format!("CREATE SCHEMA {schema}").as_str())
        .await
        .expect("create schema");
    if let Some(sql) = setup_sql {
        admin
            .execute(format!("SET search_path TO {schema}; {sql}").as_str())
            .await
            .expect("apply schema SQL");
    }
    admin.close().await;

    let schema = schema.to_string();
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .after_connect(move |conn, _meta| {
            let schema = schema.clone();
            Box::pin(async move {
                conn.execute(format!("SET search_path TO {schema}").as_str())
                    .await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .expect("connect pool");

    PgDb::new(pool)
}

/// A `PgDb` over a namespace migrated with libxmtp's real Postgres schema.
///
/// The schema is applied by running `up.sql` directly rather than through
/// `QueryMigrations::run_pending_migrations`, so the tests are pinned to the
/// committed file. The migration runner's tracking table is then seeded to match,
/// which keeps the two installation paths describing the same database — without
/// it `applied_migrations()` would report nothing here and a later
/// `run_pending_migrations()` would try to re-create every table.
pub async fn fresh_db(test_name: &str) -> PgDb {
    let setup = format!("{SCHEMA_SQL}; {}", xmtp_db::migrations::pg::baseline_sql(0));
    scoped_db(&format!("t_{test_name}"), Some(&setup)).await
}

/// A `PgDb` over an empty namespace, for tests that only exercise the executor
/// and create their own scratch tables.
pub async fn bare_db(test_name: &str) -> PgDb {
    scoped_db(&format!("b_{test_name}"), None).await
}
