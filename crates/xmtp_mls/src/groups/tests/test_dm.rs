use std::time::Duration;

use tokio_stream::StreamExt;
use tracing::info;
use xmtp_db::consent_record::StoredConsentRecord;
use xmtp_db::consent_record::{ConsentState, ConsentType};
use xmtp_db::group_message::{ContentType, MsgQueryArgs};
use xmtp_db::prelude::*;

use crate::context::XmtpSharedContext;
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
async fn test_group_update_dedupes() {
    tester!(alix);
    tester!(bo);

    let (dm, _) = alix.test_talk_in_dm_with(&bo).await?;

    let updates = || {
        dm.find_messages(&MsgQueryArgs {
            content_types: Some(vec![ContentType::GroupUpdated]),
            ..Default::default()
        })?
    };
    assert_eq!(updates().len(), 1);

    dm.update_conversation_message_disappear_from_ns(1).await?;
    assert_eq!(updates().len(), 2);

    // The same event in a row will be deduped
    dm.update_conversation_message_disappear_from_ns(1).await?;
    assert_eq!(updates().len(), 2);

    // Different time means different update, will not be deduped.
    dm.update_conversation_message_disappear_from_ns(2).await?;
    assert_eq!(updates().len(), 3);

    // Back to 1, will not be deduped because we set it to 2 and back.
    dm.update_conversation_message_disappear_from_ns(1).await?;
    assert_eq!(updates().len(), 4);

    // Continue to dedupe because the field did not change.
    dm.update_conversation_message_disappear_from_ns(1).await?;
    assert_eq!(updates().len(), 4);
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_open_dm_from_stream() {
    let dev = true;

    tester!(alix, with_dev: dev);
    for _ in 0..9 {
        tester!(_alix, from: alix);
    }
    tester!(bo, with_dev: dev);
    let mut bo = vec![bo];
    for _ in 0..9 {
        tester!(bo2, from: bo[0]);
        bo.push(bo2);
    }

    // Start up all of bo's streams.
    for bo in &bo {
        let mut stream = bo.stream_all_messages_owned_with_stats(None, None).await?;
        xmtp_common::spawn(None, async move {
            loop {
                let _ = stream.next().await;
            }
        });
    }

    // Have alix create a dm to bo
    let alix_bo_dm = alix
        .find_or_create_dm_by_inbox_id(bo[0].inbox_id(), None)
        .await?;

    // Check n times for all bo groups to be received.
    // Waiting 1 second between each check.
    for _ in 0..7 {
        xmtp_common::time::sleep(Duration::from_secs(1)).await;

        if bo.iter().all(|b| b.group(&alix_bo_dm.group_id).is_ok()) {
            return;
        }
    }

    // If we get here, that means the checks failed, some installations are missing groups.
    // Let's find out which.
    let missing: Vec<_> = bo
        .iter()
        .enumerate()
        .filter_map(|(i, b)| b.group(&alix_bo_dm.group_id).is_err().then(|| i))
        .collect();
    panic!("installations missing group: {missing:?}");
}
