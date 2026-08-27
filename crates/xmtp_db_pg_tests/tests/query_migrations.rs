//! The Postgres migration runner and the sqlx `QueryMigrations` impl.
//!
//! This is the one trait whose async form is not a port: the sync impl is
//! entirely diesel's embedded-migration machinery, which is SQLite-only. What it
//! delegates to — a tracking table plus apply/revert of embedded SQL — is
//! implemented directly here, which is what makes the Postgres backend's schema
//! deployable rather than only creatable by a test fixture.
//!
//! These tests use `bare_db` (an empty namespace) rather than `fresh_db`, so the
//! runner is exercised end to end against nothing at all.

use sqlx::Row;
use xmtp_db::migrations::QueryMigrations;
use xmtp_db::migrations::pg::{MIGRATIONS, TRACKING_TABLE};
use xmtp_db::pg::PgDb;
use xmtp_db_pg_tests::{bare_db, fresh_db};

async fn table_names(db: &PgDb) -> Vec<String> {
    let mut c = db.conn().await.unwrap();
    let rows = sqlx::query(
        "SELECT table_name FROM information_schema.tables \
         WHERE table_schema = current_schema() ORDER BY table_name",
    )
    .fetch_all(&mut *c)
    .await
    .unwrap();
    rows.iter()
        .map(|r| r.try_get::<String, _>(0).unwrap())
        .collect()
}

async fn view_exists(db: &PgDb, name: &str) -> bool {
    let mut c = db.conn().await.unwrap();
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM information_schema.views \
         WHERE table_schema = current_schema() AND table_name = $1)",
    )
    .bind(name)
    .fetch_one(&mut *c)
    .await
    .unwrap()
}

#[tokio::test]
async fn available_migrations_are_the_embedded_ones() {
    let db = bare_db("mig_available").await;
    assert_eq!(
        db.available_migrations().await.unwrap(),
        MIGRATIONS
            .iter()
            .map(|m| m.name.to_string())
            .collect::<Vec<_>>()
    );
}

/// Safe to call against a database the runner has never touched: it creates the
/// tracking table rather than failing on a missing relation.
#[tokio::test]
async fn applied_migrations_creates_its_own_tracking_table() {
    let db = bare_db("mig_tracking").await;
    assert!(db.applied_migrations().await.unwrap().is_empty());
    assert!(table_names(&db).await.contains(&TRACKING_TABLE.to_string()));

    // Idempotent.
    assert!(db.applied_migrations().await.unwrap().is_empty());
}

/// The whole point: an empty namespace becomes the real schema, and says so.
#[tokio::test]
async fn run_pending_migrations_installs_the_schema_and_records_it() {
    let db = bare_db("mig_run").await;
    let ran = db.run_pending_migrations().await.unwrap();
    assert_eq!(ran, vec!["0000".to_string()]);
    assert_eq!(db.applied_migrations().await.unwrap(), vec!["0000"]);

    let tables = table_names(&db).await;
    for expected in ["groups", "group_messages", "group_intents", "icebox"] {
        assert!(tables.contains(&expected.to_string()), "missing {expected}");
    }
    assert!(
        view_exists(&db, "conversation_list").await,
        "the view is part of the migration, not just its tables"
    );

    // Nothing pending the second time — this is what a server restart hits, and
    // re-running `up.sql` would fail on already-existing tables.
    assert!(db.run_pending_migrations().await.unwrap().is_empty());
}

/// `fresh_db` installs the schema by running `up.sql` directly. The runner must
/// agree that it is fully applied, or a server pointed at such a database would
/// try to create everything again.
#[tokio::test]
async fn the_test_fixture_and_the_runner_describe_the_same_database() {
    let db = fresh_db("mig_fixture").await;
    assert_eq!(db.applied_migrations().await.unwrap(), vec!["0000"]);
    assert!(db.run_pending_migrations().await.unwrap().is_empty());
}

#[tokio::test]
async fn rollback_reverts_and_forgets_the_migration() {
    let db = bare_db("mig_rollback").await;
    db.run_pending_migrations().await.unwrap();

    let reverted = db.rollback_to_version("0000").await.unwrap();
    assert_eq!(reverted, vec!["0000".to_string()]);
    assert!(db.applied_migrations().await.unwrap().is_empty());

    let tables = table_names(&db).await;
    assert!(!tables.contains(&"groups".to_string()));
    assert!(!view_exists(&db, "conversation_list").await);
    assert!(
        tables.contains(&TRACKING_TABLE.to_string()),
        "the tracking table outlives the migrations it tracks"
    );

    // And it can be put back.
    assert_eq!(db.run_pending_migrations().await.unwrap(), vec!["0000"]);
}

/// A target above everything applied reverts nothing.
#[tokio::test]
async fn rollback_to_a_later_version_is_a_noop() {
    let db = bare_db("mig_rollback_noop").await;
    db.run_pending_migrations().await.unwrap();

    assert!(db.rollback_to_version("9999").await.unwrap().is_empty());
    assert_eq!(db.applied_migrations().await.unwrap(), vec!["0000"]);
    assert!(table_names(&db).await.contains(&"groups".to_string()));
}

/// The version is parsed as the numeric part of the string, so a full directory
/// name works wherever a version does — matching the sync path.
#[tokio::test]
async fn a_version_may_be_written_as_its_migration_name() {
    let db = bare_db("mig_version_name").await;
    db.run_pending_migrations().await.unwrap();
    assert_eq!(
        db.rollback_to_version("0000_init").await.unwrap(),
        vec!["0000".to_string()]
    );
}

#[tokio::test]
async fn an_unparseable_version_is_rejected() {
    let db = bare_db("mig_bad_version").await;
    assert!(db.rollback_to_version("not-a-version").await.is_err());
}

/// `run_migration` and `revert_migration` are the untracked escape hatches, for
/// admin tools that want the SQL without the bookkeeping.
#[tokio::test]
async fn run_and_revert_by_name_do_not_touch_the_tracking_table() {
    let db = bare_db("mig_by_name").await;
    assert!(db.applied_migrations().await.unwrap().is_empty());

    db.run_migration("0000_init").await.unwrap();
    assert!(table_names(&db).await.contains(&"groups".to_string()));
    assert!(
        db.applied_migrations().await.unwrap().is_empty(),
        "ran, but deliberately not recorded"
    );

    db.revert_migration("0000_init").await.unwrap();
    assert!(!table_names(&db).await.contains(&"groups".to_string()));
}

#[tokio::test]
async fn an_unknown_migration_name_is_an_error() {
    let db = bare_db("mig_unknown").await;
    assert!(db.run_migration("9999_nope").await.is_err());
    assert!(db.revert_migration("9999_nope").await.is_err());
}

/// Postgres DDL is transactional, so a migration that fails partway leaves
/// nothing behind — the guarantee SQLite cannot give, and the reason the runner
/// needs no "did it get partway?" recovery.
#[tokio::test]
async fn a_failing_migration_leaves_no_partial_schema() {
    let db = bare_db("mig_atomic").await;
    // Install the schema without recording it, then try again: the second
    // attempt fails on the already-existing tables, inside the transaction.
    db.run_migration("0000_init").await.unwrap();
    // Snapshot *after* the tracking table exists. It is created outside the
    // migration transaction on purpose — it has to survive a failed migration,
    // which is the only way the next attempt can know nothing was applied.
    db.applied_migrations().await.unwrap();
    let before = table_names(&db).await;

    assert!(
        db.run_pending_migrations().await.is_err(),
        "up.sql cannot run twice"
    );
    assert_eq!(
        table_names(&db).await,
        before,
        "the failed attempt rolled back cleanly"
    );
    assert!(
        db.applied_migrations().await.unwrap().is_empty(),
        "and recorded nothing"
    );
}
