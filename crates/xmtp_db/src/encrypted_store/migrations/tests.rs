use super::*;
use crate::XmtpTestDb;
use diesel::connection::SimpleConnection;

/// A new file-backed database, holding `rows` if given, reverted until
/// `target` is its newest applied migration: what a build that stopped at
/// `target` left. Returns its path and its applied migrations.
async fn prior_database(
    target: &str,
    rows: Option<&str>,
) -> Result<(String, Vec<String>), ConnectionError> {
    let path = xmtp_common::tmp_path();
    let store = crate::TestDb::create_persistent_store(Some(path.clone())).await;
    let conn = store.db();
    if let Some(rows) = rows {
        conn.raw_query(|db| db.batch_execute(rows))?;
    }
    while conn.applied_migrations()?.first().map(String::as_str) != Some(target) {
        conn.raw_query(|db| {
            db.revert_last_migration(crate::encrypted_store::MIGRATIONS)
                .map(|_| ())
                .map_err(diesel::result::Error::QueryBuilderError)
        })?;
    }
    let applied = conn.applied_migrations()?;
    Ok((path, applied))
}

#[xmtp_common::test(unwrap_try = true)]
async fn upgrades_each_prior_database() {
    let versions = {
        let store = crate::TestDb::create_ephemeral_store().await;
        store.db().applied_migrations()?
    };
    for target in versions.iter().skip(1) {
        let (path, _) = prior_database(target, None).await?;
        {
            let store = crate::TestDb::create_persistent_store(Some(path.clone())).await;
            assert_eq!(store.db().applied_migrations()?, versions);
        }
        // The upgraded file also reopens without another schema transition.
        {
            let store = crate::TestDb::create_persistent_store(Some(path.clone())).await;
            assert_eq!(store.db().applied_migrations()?, versions);
        }
        crate::EncryptedMessageStore::<()>::remove_db_files(path);
    }
}

/// The same prior databases, holding data. One that stopped before the CBOR
/// marker was written by a bincode build: it is rejected and keeps its
/// migrations. One at or after the marker would upgrade to the latest
/// migration; none exists until a migration follows the marker.
#[xmtp_common::test(unwrap_try = true)]
async fn prior_databases_with_data_upgrade_only_from_the_cbor_marker() {
    use crate::{StorageError, XmtpDb, encrypted_store::CBOR_STORAGE_MIGRATION};

    let versions = {
        let store = crate::TestDb::create_ephemeral_store().await;
        store.db().applied_migrations()?
    };
    for target in versions.iter().skip(1) {
        let (path, prior) = prior_database(
            target,
            Some(
                "INSERT INTO openmls_key_value (key_bytes, version, value_bytes) VALUES (x'0102', 1, x'03'); \
                 INSERT INTO identity (inbox_id, installation_keys, credential_bytes) VALUES ('inbox', x'01', x'02')",
            ),
        )
        .await?;
        let database = crate::TestDb::create_database(Some(path.clone())).await;
        let result = database.init();
        let applied = DbConnection::new(database.conn()).applied_migrations()?;
        // No migration follows the marker yet, so every prior database here
        // lacks it and this branch does not run. It runs once a later
        // migration exists. Until then `a_cbor_database_runs_its_pending_migrations`
        // covers an upgrade from the marker.
        if prior
            .iter()
            .any(|version| version == CBOR_STORAGE_MIGRATION)
        {
            result?;
            assert_eq!(applied, versions, "from {target}");
        } else {
            assert!(
                matches!(result, Err(StorageError::OldEncodingDatabase)),
                "from {target}: {result:?}"
            );
            assert_eq!(applied, prior, "from {target}");
        }
        drop(database);
        crate::EncryptedMessageStore::<()>::remove_db_files(path);
    }
}

/// Upgrading discards unauthenticated remote rows and the rowid cursor that
/// would otherwise skip newly accepted entries after SQLite reuses rowids.
#[xmtp_common::test(unwrap_try = true)]
async fn the_author_migration_drops_unsigned_remote_entries() {
    use crate::prelude::*;
    use crate::refresh_state::EntityKind;
    use xmtp_proto::types::GroupId;

    let (path, _) = prior_database(
        "20261022000000",
        Some(
            "INSERT INTO remote_commit_log (log_sequence_id, group_id, commit_sequence_id, commit_result, applied_epoch_number, applied_epoch_authenticator) \
             VALUES (10, zeroblob(32), 9, 1, 2, x'01'); \
             INSERT INTO refresh_state (entity_id, entity_kind, sequence_id) VALUES (zeroblob(32), 6, 100); \
             INSERT INTO refresh_state (entity_id, entity_kind, sequence_id) VALUES (zeroblob(32), 4, 10);",
        ),
    ).await?;
    {
        let store = crate::TestDb::create_persistent_store(Some(path.clone())).await;
        let conn = store.db();
        let group_id = GroupId::from([0; 32]);
        assert!(conn.get_latest_remote_log_for_group(&group_id)?.is_none());
        assert!(
            conn.get_refresh_state(group_id, EntityKind::CommitLogForkCheckRemote)?
                .is_none()
        );
        assert_eq!(
            conn.get_last_cursor(group_id, EntityKind::CommitLogDownload)?
                .0,
            10
        );
    }
    crate::EncryptedMessageStore::<()>::remove_db_files(path);
}
