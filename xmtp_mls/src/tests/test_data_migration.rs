use crate::tester;

#[xmtp_common::test(unwrap_try = true)]
async fn setup_migration_test() {
    tester!(alix);
    tester!(bo);

    alix.test_talk_in_dm_with(&bo).await?;
    bo.test_talk_in_dm_with(&alix).await?;
    alix.test_talk_in_new_group_with(&bo).await?;

    alix.save_snapshot_to_file("alix.xmtp");
    bo.save_snapshot_to_file("bo.xmtp");
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_existing_client_db() {
    tester!(alix, snapshot_file: "alix.xmtp");
    tester!(bo, snapshot_file: "bo.xmtp");

    alix.test_talk_in_dm_with(&bo).await?;
    bo.test_talk_in_dm_with(&alix).await?;

    alix.test_talk_in_new_group_with(&bo).await?;
    bo.test_talk_in_new_group_with(&alix).await?;
}
