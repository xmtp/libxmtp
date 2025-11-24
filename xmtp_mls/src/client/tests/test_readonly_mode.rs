use crate::{context::ClientMode, tester};

#[xmtp_common::test(unwrap_try = true)]
async fn test_readonly_mode() {
    tester!(alix, mode: ClientMode::Notification);
    tester!(bo);
    let dm = alix
        .create_group_with_inbox_ids(&[bo.inbox_id()], None, None)
        .await?;

    tester!(alix2, from: alix);

    let alix_stats = alix.api_stats();

    alix_stats.send_welcome_messages.clear();
    dm.maybe_update_installations(Some(0)).await?;
    let welcomes_sent = alix_stats.send_welcome_messages.get_count();

    assert_eq!(welcomes_sent, 0);
}
