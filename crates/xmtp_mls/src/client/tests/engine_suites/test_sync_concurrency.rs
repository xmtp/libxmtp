//! The leaves of `groups/tests/test_sync_concurrency.rs` that need the client.

use crate::groups::send_message_opts::SendMessageOpts;
use crate::prelude::*;
use crate::tester;
use crate::utils::TestMlsGroup;
use crate::utils::test::FullXmtpClient;
use futures::future::join_all;
use xmtp_common::StreamHandle as _;
use xmtp_db::{group::GroupQueryArgs, group_message::MsgQueryArgs};

async fn receive_group_invite(client: &FullXmtpClient) -> TestMlsGroup {
    client.sync_welcomes().await.unwrap();
    let mut groups = client.find_groups(GroupQueryArgs::default()).unwrap();

    groups.remove(0)
}

#[xmtp_common::test(flavor = "multi_thread")]
async fn test_parallel_syncs() {
    tester!(alix1, sync_worker);

    let alix1_group = alix1.create_group(None, None).unwrap();

    tester!(alix2, from: alix1);

    let sync_tasks: Vec<_> = (0..10)
        .map(|_| {
            let group_clone = alix1_group.clone();
            // Each of these syncs is going to trigger the client to invite alix2 to the group
            // because of the race
            xmtp_common::spawn(None, async move { group_clone.sync().await }).join()
        })
        .collect();

    let results = join_all(sync_tasks).await;

    // Check if any of the syncs failed
    for result in results.into_iter() {
        assert!(result.is_ok(), "Sync error {:?}", result.err());
    }

    // Make sure that only one welcome was sent
    let alix2_welcomes = alix1
        .context
        .api()
        .query_welcome_messages(alix2.installation_public_key())
        .await
        .unwrap();
    assert_eq!(alix2_welcomes.len(), 1);

    // The installation Add is inline. The refresh emits a membership
    // AppDataUpdate proposal and the commit that consumes it.
    let group_messages = alix1
        .context
        .api()
        .query_group_messages(alix1_group.group_id)
        .await
        .unwrap();
    assert_eq!(group_messages.len(), 2);

    let alix2_group = receive_group_invite(&alix2).await;

    // Send a message from alix1
    alix1_group
        .send_message("hi from alix1".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();
    // Send a message from alix2
    alix2_group
        .send_message("hi from alix2".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();

    // Sync both clients
    alix1_group.sync().await.unwrap();
    alix2_group.sync().await.unwrap();

    let alix1_messages = alix1_group.find_messages(&MsgQueryArgs::default()).unwrap();
    let alix2_messages = alix2_group.find_messages(&MsgQueryArgs::default()).unwrap();
    assert_eq!(alix1_messages.len(), alix2_messages.len() - 1);

    assert!(
        alix1_messages
            .iter()
            .any(|m| m.decrypted_message_bytes == "hi from alix2".as_bytes())
    );
    assert!(
        alix2_messages
            .iter()
            .any(|m| m.decrypted_message_bytes == "hi from alix1".as_bytes())
    );
}
