use crate::{context::ClientMode, tester};

#[xmtp_common::test(unwrap_try = true)]
async fn test_readonly_mode() {
    tester!(alix);
    tester!(bo);

    // Have alix create a dm
    let dm = alix
        .create_group_with_inbox_ids(&[bo.inbox_id()], None, None)
        .await?;

    // Create a notif client
    tester!(alix_notif, from: alix, mode: ClientMode::Notification);

    // Have alix add the notif installation to the dm
    dm.maybe_update_installations(Some(0)).await?;
    // Have the notif client sync welcomes to receive the dm. (should succeed)
    alix_notif.sync_welcomes().await?;
    let notif_dm = alix_notif.group(&dm.group_id)?;

    // Create a second client.
    tester!(_alix2, from: alix);

    // Now we need to ensure that the notif client does not send out a welcome to alix2
    let notif_stats = alix_notif.api_stats();
    notif_stats.send_welcome_messages.clear();
    notif_dm.maybe_update_installations(Some(0)).await?;
    let welcomes_sent = notif_stats.send_welcome_messages.get_count();
    assert_eq!(welcomes_sent, 0);

    // Have bo send a message, and have the notif client receive it
    bo.sync_welcomes().await?;
    let bo_alix_dm = bo.group(&dm.group_id)?;
    bo_alix_dm
        .send_message(b"Hi there.", Default::default())
        .await?;

    notif_dm.sync().await?;
    assert_eq!(bo_alix_dm.test_last_message_bytes()??, b"Hi there.");
}
