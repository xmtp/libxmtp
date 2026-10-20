use super::*;
use crate::XmtpTestDb;

#[xmtp_common::test(unwrap_try = true)]
async fn upgrades_each_prior_database() {
    let versions = {
        let store = crate::TestDb::create_ephemeral_store().await;
        store.db().applied_migrations()?
    };
    for target in versions.iter().skip(1) {
        let path = xmtp_common::tmp_path();
        {
            let store = crate::TestDb::create_persistent_store(Some(path.clone())).await;
            let conn = store.db();
            while conn.applied_migrations()?.first() != Some(target) {
                conn.raw_query(|db| {
                    db.revert_last_migration(crate::encrypted_store::MIGRATIONS)
                        .map(|_| ())
                        .map_err(diesel::result::Error::QueryBuilderError)
                })?;
            }
        }
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
/// migrations. One at or after the marker upgrades to the latest migration.
#[xmtp_common::test(unwrap_try = true)]
async fn prior_databases_with_data_upgrade_only_from_the_cbor_marker() {
    use crate::{StorageError, XmtpDb, encrypted_store::CBOR_STORAGE_MIGRATION};
    use diesel::connection::SimpleConnection;

    let versions = {
        let store = crate::TestDb::create_ephemeral_store().await;
        store.db().applied_migrations()?
    };
    for target in versions.iter().skip(1) {
        let path = xmtp_common::tmp_path();
        let prior = {
            let store = crate::TestDb::create_persistent_store(Some(path.clone())).await;
            let conn = store.db();
            conn.raw_query(|db| {
                db.batch_execute(
                    "INSERT INTO openmls_key_value (key_bytes, version, value_bytes) VALUES (x'0102', 1, x'03'); \
                     INSERT INTO identity (inbox_id, installation_keys, credential_bytes) VALUES ('inbox', x'01', x'02')",
                )
            })?;
            while conn.applied_migrations()?.first() != Some(target) {
                conn.raw_query(|db| {
                    db.revert_last_migration(crate::encrypted_store::MIGRATIONS)
                        .map(|_| ())
                        .map_err(diesel::result::Error::QueryBuilderError)
                })?;
            }
            conn.applied_migrations()?
        };
        let database = crate::TestDb::create_database(Some(path.clone())).await;
        let result = database.init();
        let applied = DbConnection::new(database.conn()).applied_migrations()?;
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
