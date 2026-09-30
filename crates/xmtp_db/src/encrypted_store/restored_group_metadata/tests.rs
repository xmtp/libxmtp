use super::*;
use crate::{Store, XmtpTestDb, migrations::QueryMigrations};
use diesel_migrations::MigrationHarness;

// verifies: ARCH-020
#[xmtp_common::test(unwrap_try = true)]
async fn restored_group_history_reads_only_while_restored() {
    use crate::group::{QueryGroup, tests::generate_group};

    let store = crate::TestDb::create_ephemeral_store().await;
    let db = store.db();
    let group = generate_group(Some(GroupMembershipState::Restored));
    group.store(&db)?;
    let save = GroupSave {
        id: group.id.to_vec(),
        added_by_inbox_id: "archived-adder".into(),
        ..Default::default()
    };
    StoredRestoredGroupMetadata {
        group_id: group.id,
        group_save: save.encode_to_vec(),
    }
    .store(&db)?;
    assert_eq!(db.restored_group_history(&group.id)?, Some(save));

    // Activation ends the historical projection, even while the record exists.
    db.update_group_membership(group.id, GroupMembershipState::Allowed)?;
    assert_eq!(db.restored_group_history(&group.id)?, None);
    assert!(db.restored_group_metadata(&group.id)?.is_some());

    assert!(db.delete_restored_group_metadata(&group.id)?);
    assert!(!db.delete_restored_group_metadata(&group.id)?);
    assert!(db.restored_group_metadata(&group.id)?.is_none());
}

// verifies: ARCH-020
#[xmtp_common::test(unwrap_try = true)]
async fn restored_group_histories_skips_activated_and_unrecorded_rows() {
    use crate::group::{QueryGroup, tests::generate_group};

    let store = crate::TestDb::create_ephemeral_store().await;
    let db = store.db();
    let recorded = generate_group(Some(GroupMembershipState::Restored));
    let activated = generate_group(Some(GroupMembershipState::Restored));
    let unrecorded = generate_group(Some(GroupMembershipState::Restored));
    for group in [&recorded, &activated, &unrecorded] {
        group.store(&db)?;
    }
    let save = |group: &crate::group::StoredGroup| GroupSave {
        id: group.id.to_vec(),
        ..Default::default()
    };
    for group in [&recorded, &activated] {
        StoredRestoredGroupMetadata {
            group_id: group.id,
            group_save: save(group).encode_to_vec(),
        }
        .store(&db)?;
    }
    db.update_group_membership(activated.id, GroupMembershipState::Allowed)?;

    // More ids than one statement binds: unknown ids fill the first batch, so
    // the recorded group is found only in the second.
    let mut ids: Vec<GroupId> = (0..HISTORY_BATCH_SIZE)
        .map(|_| GroupId::from(xmtp_common::rand_array::<16>()))
        .collect();
    ids.extend([activated.id, unrecorded.id, recorded.id]);
    let histories = db.restored_group_histories(&ids)?;
    assert_eq!(histories, HashMap::from([(recorded.id, save(&recorded))]));
    assert!(db.restored_group_histories(&[])?.is_empty());
}

// verifies: ARCH-020
#[xmtp_common::test(unwrap_try = true)]
async fn restored_metadata_upgrades_each_prior_database() {
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
            assert!(
                store
                    .db()
                    .restored_group_metadata(&xmtp_proto::types::GroupId::from([1; 16]))?
                    .is_none()
            );
        }
        // The upgraded file also reopens without another schema transition.
        {
            let store = crate::TestDb::create_persistent_store(Some(path.clone())).await;
            assert_eq!(store.db().applied_migrations()?, versions);
        }
        crate::EncryptedMessageStore::<()>::remove_db_files(path);
    }
}
