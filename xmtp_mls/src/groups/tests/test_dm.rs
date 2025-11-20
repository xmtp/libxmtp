use std::time::Duration;

use tokio_stream::StreamExt;
use xmtp_common::NS_IN_DAY;
use xmtp_common::time::now_ns;
use xmtp_db::consent_record::StoredConsentRecord;
use xmtp_db::consent_record::{ConsentState, ConsentType};
use xmtp_db::group::{ConversationType, GroupQueryArgs};
use xmtp_db::prelude::*;

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
async fn test_old_msgs() {
    tester!(alix);
    tester!(bo);

    let dm_bo_alix = bo
        .find_or_create_dm_by_inbox_id(alix.inbox_id(), None)
        .await?;
    dm_bo_alix.send_message(b"Hi", Default::default()).await?;

    alix.sync_all_welcomes_and_groups(None).await?;
    let mut stream = alix
        .stream_all_messages_owned_with_stats(None, None)
        .await?;

    xmtp_common::spawn(None, async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        dm_bo_alix
            .send_message(b"Hi again", Default::default())
            .await?;
    });

    while let Some(msg) = stream.next().await {
        let msg = msg?;
        let txt = String::from_utf8_lossy(&msg.decrypted_message_bytes);
        tracing::error!("{txt}");
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_gm() {
    tester!(gm, snapshot_file: "../dev.db3", dev);

    tracing::info!("inbox {}", gm.inbox_id());

    gm.sync_all_welcomes_and_groups(None).await?;
    let mut stream = gm.stream_all_messages(None, None).await?;

    while let Some(v) = stream.next().await {
        let msg = String::from_utf8_lossy(&v.unwrap().decrypted_message_bytes).to_string();
        tracing::info!("{msg}");
    }
}
