//! The CBOR storage fence in `XmtpDb::init`: a database written before the
//! CBOR marker migration is rejected before any migration runs. Native and
//! wasm.

use diesel::{connection::SimpleConnection, prelude::*};
use diesel_migrations::MigrationHarness;
use xmtp_common::{ErrorCode, RetryableError};

use super::QueryMigrations;
use crate::encrypted_store::{CBOR_STORAGE_MIGRATION, EmbeddedMigrationsExt};
use crate::{ConnectionExt, DbConnection, MIGRATIONS, StorageError, TestDb, XmtpDb, XmtpTestDb};

/// A key store row as bincode wrote it: a `u64` LE length, then the bytes.
const KEY_STORE_ROW: &str = "INSERT INTO openmls_key_value (key_bytes, version, value_bytes) VALUES (x'0102', 1, x'0300000000000000aabbcc')";
const IDENTITY_ROW: &str = "INSERT INTO identity (inbox_id, installation_keys, credential_bytes) VALUES ('inbox', x'0100', x'0200')";
const BOTH_ROWS: &str = "INSERT INTO openmls_key_value (key_bytes, version, value_bytes) VALUES (x'0102', 1, x'0300000000000000aabbcc'); INSERT INTO identity (inbox_id, installation_keys, credential_bytes) VALUES ('inbox', x'0100', x'0200')";

/// Applied versions, newest first.
fn applied(conn: &mut SqliteConnection) -> Result<Vec<String>, diesel::result::Error> {
    Ok(conn
        .applied_migrations()
        .map_err(diesel::result::Error::QueryBuilderError)?
        .iter()
        .map(ToString::to_string)
        .collect())
}

/// Revert the newest migrations until `version` is no longer applied.
fn revert_through(conn: &mut SqliteConnection, version: &str) -> Result<(), diesel::result::Error> {
    while applied(conn)?.iter().any(|applied| applied == version) {
        conn.revert_last_migration(MIGRATIONS)
            .map_err(diesel::result::Error::QueryBuilderError)?;
    }
    Ok(())
}

#[derive(QueryableByName, PartialEq, Debug)]
struct KeyValue {
    #[diesel(sql_type = diesel::sql_types::Binary)]
    key_bytes: Vec<u8>,
    #[diesel(sql_type = diesel::sql_types::Binary)]
    value_bytes: Vec<u8>,
}

#[derive(QueryableByName, PartialEq, Debug)]
struct IdentityRow {
    #[diesel(sql_type = diesel::sql_types::Text)]
    inbox_id: String,
    #[diesel(sql_type = diesel::sql_types::Binary)]
    installation_keys: Vec<u8>,
    #[diesel(sql_type = diesel::sql_types::Binary)]
    credential_bytes: Vec<u8>,
}

#[derive(QueryableByName)]
struct Name {
    #[diesel(sql_type = diesel::sql_types::Text)]
    #[allow(dead_code)]
    name: String,
}

type Rows = (Vec<KeyValue>, Vec<IdentityRow>);

/// Every row of the two tables the fence reads.
fn rows(conn: &mut SqliteConnection) -> Result<Rows, diesel::result::Error> {
    Ok((
        diesel::sql_query(
            "SELECT key_bytes, value_bytes FROM openmls_key_value ORDER BY key_bytes",
        )
        .load(conn)?,
        diesel::sql_query("SELECT inbox_id, installation_keys, credential_bytes FROM identity")
            .load(conn)?,
    ))
}

/// Applied versions in Diesel's order, newest first.
fn all_versions() -> Vec<String> {
    let mut versions = MIGRATIONS.versions();
    versions.sort();
    versions.reverse();
    versions
}

/// Builds before the switch wrote databases like this one: every migration
/// except the marker, and an identity or key store row. The open fails with a
/// typed error before any migration, and no row changes.
#[xmtp_common::test(unwrap_try = true)]
async fn rejects_a_database_written_before_the_cbor_marker() {
    for data in [KEY_STORE_ROW, IDENTITY_ROW, BOTH_ROWS] {
        let database = TestDb::create_database(None).await;
        database.init()?;
        let connection = database.conn();
        connection.raw_query(|conn| {
            conn.batch_execute(data)?;
            revert_through(conn, CBOR_STORAGE_MIGRATION)
        })?;
        let before = connection.raw_query(applied)?;
        let rows_before = connection.raw_query(rows)?;
        assert!(!before.is_empty());
        assert!(
            !before
                .iter()
                .any(|version| version == CBOR_STORAGE_MIGRATION)
        );
        assert!(!rows_before.0.is_empty() || !rows_before.1.is_empty());

        let error = match database.init() {
            Err(error) => error,
            Ok(()) => panic!("a database with {data} opened without the CBOR marker"),
        };
        assert!(
            matches!(error, StorageError::OldEncodingDatabase),
            "{error}"
        );
        assert_eq!(error.error_code(), "StorageError::OldEncodingDatabase");
        assert!(!error.is_retryable());
        assert_eq!(
            error.to_string(),
            "This database uses an older storage encoding. Keep a backup and create a new client database."
        );

        assert_eq!(connection.raw_query(applied)?, before);
        assert_eq!(connection.raw_query(rows)?, rows_before);
        // A second open gives the same answer.
        assert!(matches!(
            database.init(),
            Err(StorageError::OldEncodingDatabase)
        ));
    }
}

/// A new database applies the marker with every other migration, and it
/// reopens once it holds data.
#[xmtp_common::test(unwrap_try = true)]
async fn a_new_database_applies_the_cbor_marker_and_reopens() {
    assert!(
        MIGRATIONS
            .versions()
            .contains(&CBOR_STORAGE_MIGRATION.to_string()),
        "the marker constant names no embedded migration"
    );
    let database = TestDb::create_database(None).await;
    database.init()?;
    let connection = database.conn();
    assert!(
        connection
            .raw_query(applied)?
            .iter()
            .any(|version| version == CBOR_STORAGE_MIGRATION)
    );
    connection.raw_query(|conn| conn.batch_execute(BOTH_ROWS))?;
    database.init()?;
    assert_eq!(connection.raw_query(applied)?, all_versions());
}

/// Diesel commits each migration on its own. A new database whose first open
/// stopped before the marker has some migrations and no data, and its next
/// open finishes the migrations.
#[xmtp_common::test(unwrap_try = true)]
async fn a_first_open_that_stopped_before_the_marker_finishes() {
    let versions = MIGRATIONS.versions();
    // Stopped just before the marker, and stopped right after the baseline.
    for stop_after in [versions.len() - 2, 0] {
        let database = TestDb::create_database(None).await;
        database.init()?;
        let connection = database.conn();
        connection.raw_query(|conn| revert_through(conn, &versions[stop_after + 1]))?;
        assert_eq!(connection.raw_query(applied)?.len(), stop_after + 1);

        database.init()?;
        assert_eq!(connection.raw_query(applied)?, all_versions());
    }
}

/// A database that has the marker passes the fence with its data, and init
/// still runs its pending migrations. No migration follows the marker yet, so
/// an earlier one is made pending: its revert runs and its record goes.
#[xmtp_common::test(unwrap_try = true)]
async fn a_cbor_database_runs_its_pending_migrations() {
    const GROUP_REFERENCE: &str = "2026-10-09-020000_group_reference";
    const GROUP_REFERENCE_VERSION: &str = "20261009020000";
    const INDEX: &str = "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'group_messages_group_reference'";

    let database = TestDb::create_database(None).await;
    database.init()?;
    let connection = database.conn();
    connection.raw_query(|conn| conn.batch_execute(BOTH_ROWS))?;
    let rows_before = connection.raw_query(rows)?;

    DbConnection::new(database.conn()).revert_migration(GROUP_REFERENCE)?;
    connection.raw_query(|conn| {
        diesel::sql_query("DELETE FROM __diesel_schema_migrations WHERE version = ?")
            .bind::<diesel::sql_types::Text, _>(GROUP_REFERENCE_VERSION)
            .execute(conn)
    })?;
    assert!(
        !connection
            .raw_query(applied)?
            .iter()
            .any(|version| version == GROUP_REFERENCE_VERSION)
    );
    assert!(
        connection
            .raw_query(|conn| diesel::sql_query(INDEX).load::<Name>(conn))?
            .is_empty()
    );

    database.init()?;
    assert_eq!(connection.raw_query(applied)?, all_versions());
    assert_eq!(
        connection
            .raw_query(|conn| diesel::sql_query(INDEX).load::<Name>(conn))?
            .len(),
        1
    );
    assert_eq!(connection.raw_query(rows)?, rows_before);
}
