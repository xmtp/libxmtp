use std::sync::Arc;

use crate::{context::ClientMode, tester};

#[xmtp_common::test(unwrap_try = true)]
async fn test_readonly_mode() {
    tester!(alix);
    tester!(alix2, from: alix);
    tester!(bo);

    // Have alix create a dm
    let alix_group = alix
        .create_group_with_inbox_ids(&[bo.inbox_id()], None, None)
        .await?;

    // Create a notif client
    let alix_snap = Arc::new(alix2.db_snapshot());
    tester!(alix_notif, snapshot: alix_snap, mode: ClientMode::Readonly);
    let notif_stats = alix_notif.api_stats();

    // Have alix add the notif installation to the dm
    alix_group.maybe_update_installations(Some(0)).await?;
    // Have the notif client sync welcomes to receive the dm. (should succeed)
    alix_notif.sync_welcomes().await?;
    let notif_group = alix_notif.group(&alix_group.group_id)?;

    // Create a second client.
    tester!(_alix3, from: alix);

    // Now we need to ensure that the notif client does not send out a welcome to alix2
    notif_stats.send_welcome_messages.clear();
    notif_group.maybe_update_installations(Some(0)).await?;
    let welcomes_sent = notif_stats.send_welcome_messages.get_count();
    assert_eq!(welcomes_sent, 0);

    // Have bo send a message, and have the notif client receive it
    bo.sync_welcomes().await?;
    let bo_alix_group = bo.group(&alix_group.group_id)?;
    bo_alix_group
        .send_message(b"Hi there.", Default::default())
        .await?;

    // Notif client should receive the message fine.
    notif_group.sync().await?;
    assert_eq!(notif_group.test_last_message_bytes()??, b"Hi there.");

    // Add Caro to the group, update the name, and then send a message
    tester!(caro);
    alix_group
        .add_members_by_inbox_id(&[caro.inbox_id()])
        .await?;
    alix_group
        .update_group_name("New group name!".to_string())
        .await?;
    alix_group
        .send_message(b"Hello again", Default::default())
        .await?;

    notif_group.sync().await?;

    assert_eq!(notif_group.test_last_message_bytes()??, b"Hello again");
    assert_eq!(notif_stats.send_welcome_messages.get_count(), 0);
    assert_eq!(notif_stats.upload_key_package.get_count(), 0);
    assert_eq!(notif_stats.send_group_messages.get_count(), 0);
    assert_eq!(notif_stats.publish_commit_log.get_count(), 0);

    // Now we want to pass the snapshot of the notif client back to a normal client
    // to ensure it can continue to function as normal.
    let notif_snap = alix_notif.db_snapshot();
    drop(alix_notif);
    tester!(alix, snapshot: Arc::new(notif_snap));

    // Run a sync.
    alix.sync_all_welcomes_and_groups(None).await?;
    tester!(derek);

    // Get the group, add derek.
    let alix_group = alix.group(&alix_group.group_id)?;
    alix_group
        .add_members_by_inbox_id(&[derek.inbox_id()])
        .await?;

    // Have derek receive the group
    derek.sync_welcomes().await?;
    let derek_group = derek.group(&alix_group.group_id)?;

    // Have alix send a message.
    alix_group
        .send_message(b"I am alive", Default::default())
        .await?;
    derek_group.sync().await?;
    assert_eq!(derek_group.test_last_message_bytes()??, b"I am alive");
}
