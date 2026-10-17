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
