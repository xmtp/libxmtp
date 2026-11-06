//! The leaves of `groups/tests/test_sync_concurrency.rs` that need the client.

use crate::groups::send_message_opts::SendMessageOpts;
use crate::prelude::*;
use crate::tester;
use crate::utils::TestMlsGroup;
use crate::utils::test::FullXmtpClient;
use futures::future::join_all;
use prost::Message;
use xmtp_common::StreamHandle as _;
use xmtp_db::{group::GroupQueryArgs, group_message::MsgQueryArgs};
use xmtp_proto::types::{Cursor, Topic};

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

#[xmtp_common::test(flavor = "multi_thread")]
async fn can_stream_out_of_order_without_forking() {
    tester!(client_a1);
    tester!(client_b);
    tester!(client_c);

    // Create a group
    let group_a = client_a1.create_group(None, None).unwrap();

    // Add client_b and client_c to the group
    group_a
        .add_members(&[client_b.inbox_id(), client_c.inbox_id()])
        .await
        .unwrap();

    // Sync the group
    client_b.sync_welcomes().await.unwrap();
    let binding = client_b.find_groups(GroupQueryArgs::default()).unwrap();
    let group_b = binding.first().unwrap();

    client_c.sync_welcomes().await.unwrap();
    let binding = client_c.find_groups(GroupQueryArgs::default()).unwrap();
    let group_c = binding.first().unwrap();

    // Each client sends a message and syncs (ensures any key update commits are sent)
    group_a
        .send_message_optimistic("Message a1".as_bytes(), SendMessageOpts::default())
        .unwrap();
    crate::groups::test_support::publish_intents(&group_a)
        .await
        .unwrap();

    group_a.sync().await.unwrap();
    group_b.sync().await.unwrap();
    group_c.sync().await.unwrap();

    group_b
        .send_message_optimistic("Message b1".as_bytes(), SendMessageOpts::default())
        .unwrap();
    crate::groups::test_support::publish_intents(group_b)
        .await
        .unwrap();

    group_a.sync().await.unwrap();
    group_b.sync().await.unwrap();
    group_c.sync().await.unwrap();

    group_c
        .send_message_optimistic("Message c1".as_bytes(), SendMessageOpts::default())
        .unwrap();
    crate::groups::test_support::publish_intents(group_c)
        .await
        .unwrap();

    // Sync the groups
    group_a.sync().await.unwrap();
    group_b.sync().await.unwrap();
    group_c.sync().await.unwrap();

    // After client a adds b and c, and they each sent a message, all groups are in the same epoch
    assert_eq!(group_a.epoch().await.unwrap(), 3);
    assert_eq!(group_b.epoch().await.unwrap(), 3);
    assert_eq!(group_c.epoch().await.unwrap(), 3);

    // Client b updates the group name, (incrementing the epoch from 3 to 4), and syncs
    group_b
        .update_group_name("Group B".to_string())
        .await
        .unwrap();
    group_b.sync().await.unwrap();

    // Client c sends two text messages before incrementing the epoch
    group_c
        .send_message_optimistic("Message c2".as_bytes(), SendMessageOpts::default())
        .unwrap();
    crate::groups::test_support::publish_intents(group_c)
        .await
        .unwrap();
    group_b.sync().await.unwrap();

    // Retrieve all messages from group B, verify they contain the two messages from client c even though they were sent from the wrong epoch
    let messages = client_b
        .context
        .api()
        .query_group_messages(group_b.group_id)
        .await
        .unwrap();
    // Adding B and C emits two Add proposals, one membership AppDataUpdate proposal,
    // and their commit. The remaining eight envelopes are the messages and later commits.
    assert_eq!(messages.len(), 12);

    // Get reference to last message
    let last_message = messages.last().unwrap();

    // A notification fetches and processes its complete ordered prefix.
    let wire = group_a
        .context
        .api()
        .query_all(
            [(
                xmtp_proto::types::Topic::new_group_message(group_a.group_id),
                Cursor(0),
            )]
            .into(),
            group_a.context.api().limits().max_query_limit as u32,
        )
        .await
        .unwrap()
        .into_iter()
        .find(|envelope| {
            envelope
                .meta
                .as_ref()
                .and_then(|meta| meta.cursor.as_ref())
                .is_some_and(|cursor| cursor.sequence_id == last_message.cursor.0)
        })
        .unwrap();
    let result =
        crate::subscriptions::process_streamed_group_message(&group_a, wire.encode_to_vec()).await;
    assert!(result.is_ok());

    // All three installations must process the known last envelope before
    // comparing epochs. A sampled network head can lag this envelope.
    for group in [&group_a, group_b, group_c] {
        crate::subscriptions::barrier::wait_through(
            &group.context,
            [(
                Topic::new_group_message(group.group_id),
                last_message.cursor,
            )]
            .into(),
            None,
        )
        .await
        .unwrap();
    }

    assert_eq!(group_b.epoch().await.unwrap(), 4);
    assert_eq!(group_c.epoch().await.unwrap(), 4);
    assert_eq!(group_a.epoch().await.unwrap(), 4);
}
