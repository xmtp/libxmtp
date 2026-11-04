//! The leaves of `groups/mls_sync/receive.rs` that need the client.

use crate::groups::test_support::group_updates::{
    PAGE_SIZE, add_update, encode, seed_tied_updates,
};
use crate::tester;
use xmtp_db::ConnectionExt;
use xmtp_db::diesel::{ExpressionMethods, RunQueryDsl};
use xmtp_db::group_message::{ContentType, MsgQueryArgs};
use xmtp_proto::xmtp::mls::message_contents::GroupUpdated;

#[xmtp_common::test(unwrap_try = true)]
async fn test_startup_preserves_dm_update_history() {
    use xmtp_db::user_preferences::StoredUserPreferences;
    use xmtp_proto::xmtp::mls::message_contents::group_updated::MetadataFieldChange;

    tester!(alix);
    tester!(bo);
    let (dm, _) = alix.test_talk_in_dm_with(&bo).await?;
    let query = MsgQueryArgs {
        content_types: Some(vec![ContentType::GroupUpdated]),
        ..Default::default()
    };
    let old_count = dm.find_messages_v2(&query)?.len();
    let metadata = GroupUpdated {
        metadata_field_changes: vec![MetadataFieldChange {
            field_name: "expiration_setting".into(),
            old_value: None,
            new_value: Some("123".into()),
        }],
        ..Default::default()
    };
    let mut rows = vec![encode(&metadata); 3];
    rows.extend(vec![
        encode(&add_update("repeated"));
        PAGE_SIZE as usize + 25
    ]);
    seed_tied_updates(&alix.db(), dm.group_id, rows);
    let before = dm.find_messages_v2(&query)?;
    assert_eq!(before.len(), old_count + PAGE_SIZE as usize + 28);
    alix.db().raw_query(|conn| {
        xmtp_db::diesel::update(xmtp_db::schema::user_preferences::table)
            .set(xmtp_db::schema::user_preferences::dm_group_updates_migrated.eq(false))
            .execute(conn)
    })?;
    let _reopened = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    // Bound the observation: startup must no longer complete this migration.
    let _ = xmtp_common::time::timeout(
        std::time::Duration::from_secs(1),
        xmtp_common::wait_for_eq(
            || async {
                StoredUserPreferences::load(alix.db())
                    .unwrap()
                    .dm_group_updates_migrated
            },
            true,
        ),
    )
    .await;
    let after = dm.find_messages_v2(&query)?;
    assert_eq!(
        after.len(),
        before.len(),
        "startup must preserve all DM update history"
    );
    for message in before {
        assert!(
            after
                .iter()
                .any(|row| row.metadata.id == message.metadata.id)
        );
    }
}
