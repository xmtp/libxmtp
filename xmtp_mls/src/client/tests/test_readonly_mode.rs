use crate::{context::ClientMode, tester};

#[xmtp_common::test(unwrap_try = true)]
async fn test_readonly_mode() {
    tester!(alix);
    tester!(bo);

    // Have alix create a dm
    let alix_group = alix
        .create_group_with_inbox_ids(&[bo.inbox_id()], None, None)
        .await?;

    // Create a notif client
    tester!(alix_notif, from: alix, mode: ClientMode::Notification);

    // Have alix add the notif installation to the dm
    alix_group.maybe_update_installations(Some(0)).await?;
    // Have the notif client sync welcomes to receive the dm. (should succeed)
    alix_notif.sync_welcomes().await?;
    let notif_group = alix_notif.group(&alix_group.group_id)?;

    // Create a second client.
    tester!(_alix2, from: alix);

    // Now we need to ensure that the notif client does not send out a welcome to alix2
    let notif_stats = alix_notif.api_stats();
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
    let welcomes_sent = notif_stats.send_welcome_messages.get_count();
    assert_eq!(welcomes_sent, 0);
}
