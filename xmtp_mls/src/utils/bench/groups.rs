//! Group benchmark utilities

use crate::groups::{MlsGroup, send_message_opts::SendMessageOpts};
use crate::utils::TestXmtpMlsContext;
use indicatif::{ProgressBar, ProgressStyle};
use prost::Message;
use std::sync::Arc;
use xmtp_content_types::test_utils::TestContentGenerator;
use xmtp_db::encrypted_store::group_message::{MsgQueryArgs, SortDirection};

use super::{BenchClient, Identity};

/// Setup data for group benchmarks
pub struct GroupBenchSetup {
    pub client: BenchClient,
    pub target_groups: Vec<MlsGroup<TestXmtpMlsContext>>,
    pub total_groups: usize,
}

/// Create a specified number of groups with one message each using optimistic sends
/// Returns the client and a list of the target groups to find
pub async fn setup_groups_with_messages(
    client: BenchClient,
    total_groups: usize,
    target_groups: usize,
) -> GroupBenchSetup {
    let style =
        ProgressStyle::with_template("{bar} {pos}/{len} elapsed {elapsed} remaining {eta_precise}");
    let bar = ProgressBar::new(total_groups as u64).with_style(style.unwrap());
    bar.set_message("Creating groups and sending messages");

    let mut all_groups = Vec::with_capacity(total_groups);
    let mut target_group_list = Vec::with_capacity(target_groups);

    // Create all groups and send one message to each (using optimistic send)
    for i in 0..total_groups {
        let group = client.create_group(None, None).unwrap();

        // Send a message to the group using optimistic send with proper EncodedContent
        let content = TestContentGenerator::text_content(&format!("Test message {}", i));
        group
            .send_message_optimistic(
                content.encode_to_vec().as_slice(),
                SendMessageOpts::default(),
            )
            .unwrap();

        // Keep track of the first `target_groups` groups as our targets
        if i < target_groups {
            target_group_list.push(group.clone());
        }

        all_groups.push(group);
        bar.inc(1);
    }

    bar.finish_with_message("Groups created and messages sent");

    GroupBenchSetup {
        client,
        target_groups: target_group_list,
        total_groups,
    }
}

/// Create multiple clients with pre-generated identities for group operations
pub async fn setup_clients_from_identities(
    identities: &[Identity],
    is_dev_network: bool,
) -> Vec<BenchClient> {
    let mut clients = Vec::with_capacity(identities.len());

    for identity in identities.iter().take(100) {
        // Limit to avoid resource issues
        let client = super::clients::create_client_from_identity(identity, is_dev_network).await;
        clients.push(client);
    }

    clients
}

/// Setup data for message benchmarks
pub struct MessageBenchSetup {
    pub client: BenchClient,
    pub group: MlsGroup<TestXmtpMlsContext>,
    pub total_messages: usize,
    pub earliest_message_timestamp: i64,
    pub latest_message_timestamp: i64,
}

/// Create a group with a specified number of messages using optimistic sends with proper EncodedContent
pub async fn setup_group_with_messages(
    client: BenchClient,
    total_messages: usize,
) -> Arc<MessageBenchSetup> {
    let style =
        ProgressStyle::with_template("{bar} {pos}/{len} elapsed {elapsed} remaining {eta_precise}");
    let bar = ProgressBar::new(total_messages as u64).with_style(style.unwrap());
    bar.set_message("Creating group and sending messages");

    let group = client.create_group(None, None).unwrap();

    // Send messages using optimistic send with proper EncodedContent
    for i in 0..total_messages {
        let content = TestContentGenerator::text_content(&format!("Test message {}", i));
        group
            .send_message_optimistic(&content.encode_to_vec(), SendMessageOpts::default())
            .unwrap();
        bar.inc(1);
    }

    bar.finish_with_message("Group created and messages sent");

    // Query the actual message timestamps from the database to get realistic filter values
    // Get earliest timestamp (ascending order, limit 1)
    let earliest_messages = group
        .find_messages(&MsgQueryArgs {
            limit: Some(1),
            direction: Some(SortDirection::Ascending),
            ..Default::default()
        })
        .unwrap();

    // Get latest timestamp (descending order, limit 1)
    let latest_messages = group
        .find_messages(&MsgQueryArgs {
            limit: Some(1),
            direction: Some(SortDirection::Descending),
            ..Default::default()
        })
        .unwrap();

    let earliest_message_timestamp = earliest_messages.first().map(|m| m.sent_at_ns).unwrap_or(0);
    let latest_message_timestamp = latest_messages.first().map(|m| m.sent_at_ns).unwrap_or(0);

    Arc::new(MessageBenchSetup {
        client,
        group,
        total_messages,
        earliest_message_timestamp,
        latest_message_timestamp,
    })
}
