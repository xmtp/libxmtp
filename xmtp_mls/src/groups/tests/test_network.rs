use std::time::Duration;

use crate::tester;
use toxiproxy_rust::toxic::ToxicPack;
use xmtp_db::group::GroupQueryArgs;

#[xmtp_common::test(unwrap_try = true)]
async fn test_network_drop() {
    tester!(alix, proxy);
    tester!(bo);

    // Cut the network connection and make a group with members
    alix.proxy().disable().await?;
    let result = alix
        .create_group_with_inbox_ids(&[bo.inbox_id()], None, None)
        .await;

    // The group should be created, but an error should be reported from trying to add members
    // without a network connection.
    assert!(result.is_err());

    // The group should still be created, even though the add members request didn't go through.
    let g = alix.find_groups(GroupQueryArgs::default())?.pop().unwrap();

    // Bo should not have received the welcome for the group.
    bo.sync_welcomes().await?;
    assert!(bo.group(&g.group_id).is_err());

    // Turn alix's connection back on.
    alix.proxy().enable().await?;
    // Try adding bo again.
    g.add_members_by_inbox_id(&[bo.inbox_id()]).await?;

    // Bo should get the welcome for the group.
    bo.sync_welcomes().await?;
    assert!(bo.group(&g.group_id).is_ok());
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_stream_drop() {
    tester!(alix, proxy, stream);
    tester!(bo);

    let (alix_group, mut other_groups) = alix.test_talk_in_new_group_with(&[&bo]).await?;
    let bo_group = other_groups.pop()?;

    // Test that alix is streaming messages from bo
    bo_group.send_message(b"Hi").await?;
    assert!(alix_group.test_last_message_eq(b"Hi")?);

    // Cut the network connection and make a group with members
    alix.proxy().disable().await?;
    // The connection is severed. Alix should not be streaming messages from bo.
    bo_group.send_message(b"Ho").await?;
    assert!(!alix_group.test_last_message_eq(b"Ho")?);

    tokio::time::sleep(Duration::from_secs(5)).await;

    // Turn the connection back on.
    alix.proxy().enable().await?;

    // The stream should reconnect and start streaming messages from bo again.
    bo_group.send_message(b"Hi").await?;
    assert!(alix_group.test_last_message_eq(b"Hi")?);
}
