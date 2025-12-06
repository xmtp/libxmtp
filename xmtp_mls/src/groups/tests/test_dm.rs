use std::time::{Duration, Instant};

use tokio_stream::StreamExt;
use xmtp_db::consent_record::StoredConsentRecord;
use xmtp_db::consent_record::{ConsentState, ConsentType};
use xmtp_db::prelude::*;

use crate::subscriptions::stream_messages::stream_stats::StreamWithStats;
use crate::tester;

/// Test case: If two users are talking in a DM, and one user
/// creates a new installation and creates a new DM before being
/// welcomed into the old DM, that new DM group should be consented.
#[xmtp_common::test(unwrap_try = true)]
async fn auto_consent_dms_for_new_installations() {
    tester!(alix);
    tester!(bo1);
    // Alix and bo are talking fine in a DM
    alix.test_talk_in_dm_with(&bo1).await?;

    tester!(bo2, from: bo1);

    // Bo creates a new installation and immediately creates a new DM with alix
    let bo2_dm = bo2
        .find_or_create_dm_by_inbox_id(alix.inbox_id(), None)
        .await?;

    // Alix pulls down the new DM from bo
    alix.sync_welcomes().await?;

    // That DM should be already consented, since alix consented with bo in another DM
    let consent = alix
        .get_consent_state(ConsentType::ConversationId, hex::encode(bo2_dm.group_id))
        .await?;
    assert_eq!(consent, ConsentState::Allowed);
}

/// Test case: If a second installation syncs the consent state for a DM
/// before processing the welcome, the welcome should succeed rather than
/// aborting on a unique constraint error.
#[xmtp_common::test(unwrap_try = true)]
async fn test_dm_welcome_with_preexisting_consent() {
    tester!(alix);
    tester!(bo1);
    // Alix and bo are talking fine in a DM
    let (a_group, _) = alix.test_talk_in_dm_with(&bo1).await?;

    tester!(bo2, from: bo1);

    // Mock device sync - the consent record is processed on Bo2 before
    // the welcome is processed.
    let cr = StoredConsentRecord::new(
        ConsentType::ConversationId,
        ConsentState::Allowed,
        hex::encode(&a_group.group_id),
    );
    bo2.context.db().insert_newer_consent_record(cr)?;
    // Now bo2 processes the welcome
    bo1.find_or_create_dm_by_inbox_id(alix.inbox_id(), None)
        .await?
        .update_installations()
        .await?;
    bo2.sync_welcomes().await?;

    // The welcome should succeed
    assert_eq!(
        bo2.find_or_create_dm_by_inbox_id(alix.inbox_id(), None)
            .await?
            .group_id,
        a_group.group_id
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_setup_test() {
    tester!(alix);
    for _ in 0..300 {
        tester!(bo, disable_workers);
        bo.test_talk_in_dm_with(&alix).await?;
        bo.test_talk_in_new_group_with(&alix).await?;
    }

    alix.sync_all_welcomes_and_groups(None).await?;

    let snap = alix.db_snapshot();
    std::fs::write("alix.db3", snap)?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_logs_issue() {
    tester!(alix, snapshot_file: "alix.db3");
    tester!(bo);

    let mut stream = alix
        .stream_all_messages_owned_with_stats(None, None)
        .await?;
    let stats = stream.stats();

    tokio::task::spawn(async move { while let Some(_) = stream.next().await {} });

    let opt_group = alix.create_group(Default::default(), Default::default())?;
    // opt_group.add_members_by_inbox_id(&[bo.inbox_id()]).await?;
    // let msg = opt_group.send_message_optimistic(b"Hi group", Default::default())?;

    // let new_stats = stats.new_stats().await;
    // tracing::info!("new stats: {new_stats:?}");

    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(100) {
        let new_stats = stats.new_stats().await;
        tracing::info!("New stats: {}", new_stats.len());
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
