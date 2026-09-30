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
            use diesel::prelude::*;
            let rows = store.db().raw_query(|conn| {
                crate::schema::restored_group_metadata::table
                    .count()
                    .get_result::<i64>(conn)
            })?;
            assert_eq!(rows, 0);
        }
        // The upgraded file also reopens without another schema transition.
        {
            let store = crate::TestDb::create_persistent_store(Some(path.clone())).await;
            assert_eq!(store.db().applied_migrations()?, versions);
        }
        crate::EncryptedMessageStore::<()>::remove_db_files(path);
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn reopening_keeps_parked_archive_metadata() {
    use crate::{Store, group::tests::generate_group, schema::restored_group_metadata as history};
    use diesel::prelude::*;

    let path = xmtp_common::tmp_path();
    let group = generate_group(None);
    let archived = vec![0x80, 0xff];
    {
        let store = crate::TestDb::create_persistent_store(Some(path.clone())).await;
        group.store(&store.db())?;
        store.db().raw_query(|conn| {
            diesel::insert_into(history::table)
                .values((
                    history::group_id.eq(group.id),
                    history::group_save.eq(&archived),
                ))
                .execute(conn)
        })?;
    }
    {
        let store = crate::TestDb::create_persistent_store(Some(path.clone())).await;
        let saved: Vec<u8> = store.db().raw_query(|conn| {
            history::table
                .find(group.id)
                .select(history::group_save)
                .first(conn)
        })?;
        assert_eq!(saved, archived);
        assert!(store.db().run_pending_migrations()?.is_empty());
    }
    crate::EncryptedMessageStore::<()>::remove_db_files(path);
}
