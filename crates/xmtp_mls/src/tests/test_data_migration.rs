use crate::tester;

const ALIX_DB: &str = "tests/assets/alix.xmtp";
const BO_DB: &str = "tests/assets/bo.xmtp";

#[xmtp_common::test(unwrap_try = true)]
#[ignore]
async fn setup_migration_test() {
    tester!(alix);
    tester!(bo);

    alix.test_talk_in_dm_with(&bo).await?;
    bo.test_talk_in_dm_with(&alix).await?;
    alix.test_talk_in_new_group_with(&bo).await?;

    alix.save_snapshot_to_file(ALIX_DB);
    bo.save_snapshot_to_file(BO_DB);
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_existing_client_db() {
    tester!(alix, snapshot_file: ALIX_DB);
    tester!(bo, snapshot_file: BO_DB);
    tester!(caro);

    // TODO: create an endpoint on the server to clear a topic
    // in testing. This works the first time, but cannot be re-run.
    // Commenting out for now to avoid complications.
    // alix.test_talk_in_dm_with(&bo).await?;
    // bo.test_talk_in_dm_with(&alix).await?;

    alix.test_talk_in_new_group_with(&bo).await?;
    bo.test_talk_in_new_group_with(&alix).await?;

    alix.test_talk_in_dm_with(&caro).await?;
    alix.test_talk_in_new_group_with(&caro).await?;
    bo.test_talk_in_dm_with(&caro).await?;
    bo.test_talk_in_new_group_with(&caro).await?;
}
