//! Group benchmark utilities

use crate::groups::MlsGroup;
use crate::utils::TestXmtpMlsContext;
use indicatif::{ProgressBar, ProgressStyle};

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

        // Send a message to the group using optimistic send (no network round trip)
        group
            .send_message_optimistic(format!("Test message {}", i).as_bytes())
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
