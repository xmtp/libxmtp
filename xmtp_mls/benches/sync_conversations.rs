//! Benchmarks for sync_all_conversations operations with fresh setup per iteration
//!
//! This benchmark measures the performance of sync_all_groups under various conditions:
//! - 10 groups, 1 with new messages
//! - 10 groups, 10 with new messages
//! - 100 groups, 10 with new messages
//! - 100 groups, 100 with new messages
//!
//! Each scenario creates groups where other clients send messages that the main client
//! hasn't seen yet, creating a realistic sync scenario. Each group with messages receives
//! 2 messages from other clients that need to be synced.
//!
//! IMPORTANT: This benchmark requires fresh setup for each iteration because once groups
//! are synced, subsequent sync operations will have different performance characteristics.

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use indicatif::{ProgressBar, ProgressStyle};
use prost::Message;
use std::{hint::black_box, sync::Arc, time::Duration};
use tokio::runtime::{Builder, Runtime};
use tracing::{Instrument, trace_span};
use xmtp_common::bench::{self, BENCH_ROOT_SPAN, bench_async_setup};
use xmtp_content_types::test_utils::TestContentGenerator;
use xmtp_mls::groups::MlsGroup;
use xmtp_mls::groups::send_message_opts::SendMessageOpts;
use xmtp_mls::utils::TestXmtpMlsContext;
use xmtp_mls::utils::bench::{BenchClient, new_client};

pub const SAMPLE_SIZE: usize = 10;

/// Test scenarios for sync_all_conversations benchmarks
/// (total_groups, groups_with_messages, expected_synced_count)
const SYNC_SCENARIOS: [(usize, usize, usize); 4] = [
    (10, 1, 1),      // 10 groups, 1 with new messages
    (10, 10, 10),    // 10 groups, 10 with new messages
    (100, 10, 10),   // 100 groups, 10 with new messages
    (100, 100, 100), // 100 groups, 100 with new messages
];

fn setup_runtime() -> Runtime {
    Builder::new_multi_thread()
        .enable_time()
        .enable_io()
        .thread_name("xmtp-sync-conversations-bencher")
        .build()
        .unwrap()
}

/// Setup data for sync conversations benchmarks
pub struct SyncConversationsBenchSetup {
    pub client: BenchClient,
    pub other_client: BenchClient,
    pub groups_with_messages: Vec<MlsGroup<TestXmtpMlsContext>>,
    pub all_groups: Vec<MlsGroup<TestXmtpMlsContext>>,
    pub total_groups: usize,
    pub expected_synced_count: usize,
}

/// Create a setup with the specified number of groups, some with unsynced messages
async fn setup_sync_conversations_bench(
    total_groups: usize,
    groups_with_messages: usize,
    expected_synced_count: usize,
) -> Arc<SyncConversationsBenchSetup> {
    let client = new_client(false).await;
    let other_client = new_client(false).await;

    let style =
        ProgressStyle::with_template("{bar} {pos}/{len} elapsed {elapsed} remaining {eta_precise}");
    let bar = ProgressBar::new(total_groups as u64).with_style(style.unwrap());
    bar.set_message("Creating groups for sync benchmark");

    let mut all_groups = Vec::with_capacity(total_groups);
    let mut groups_with_new_messages = Vec::with_capacity(groups_with_messages);
    let mut other_client_groups = Vec::with_capacity(total_groups);

    // Create all groups and add other_client as a member
    for i in 0..total_groups {
        let group = client.create_group(None, None).unwrap();

        // Add other_client to the group
        group
            .add_members_by_inbox_id(&[other_client.inbox_id()])
            .await
            .unwrap();

        all_groups.push(group.clone());
        other_client_groups.push(group.group_id.clone());

        // Mark the first `groups_with_messages` groups as having new messages
        if i < groups_with_messages {
            groups_with_new_messages.push(group);
        }

        bar.inc(1);
    }

    bar.finish_with_message("Groups created and members added");
    // Sync the group on other_client so it can send messages
    let synced_groups = other_client
        .sync_all_welcomes_and_groups(None)
        .await
        .unwrap();
    assert!(
        synced_groups > 0,
        "Other client should have received the group"
    );

    // Have other_client send messages to selected groups
    // Each group gets 2 messages - these will be unsynced from the main client's perspective
    let message_bar = ProgressBar::new(groups_with_messages as u64);
    message_bar.set_message("Other client sending messages to groups");

    for (i, other_group_id) in other_client_groups
        .iter()
        .enumerate()
        .take(groups_with_messages)
    {
        let other_group = other_client
            .find_groups(Default::default())
            .unwrap()
            .into_iter()
            .find(|g| g.group_id == *other_group_id)
            .expect("Other client should have the group");

        // Send 2 messages from other_client - these will need to be synced by main client
        let content1 =
            TestContentGenerator::text_content(&format!("Sync message {} from other", i));

        other_group
            .send_message(&content1.encode_to_vec(), SendMessageOpts::default())
            .await
            .unwrap();
        message_bar.inc(1);
    }

    message_bar.finish_with_message("Messages sent by other client");

    Arc::new(SyncConversationsBenchSetup {
        client,
        other_client,
        groups_with_messages: groups_with_new_messages,
        all_groups,
        total_groups,
        expected_synced_count,
    })
}

fn bench_sync_all_conversations(c: &mut Criterion) {
    bench::logger();
    let mut benchmark_group = c.benchmark_group("sync_all_conversations");
    benchmark_group.sample_size(SAMPLE_SIZE);
    benchmark_group.measurement_time(Duration::from_secs(30)); // Reduced from 60s
    benchmark_group.warm_up_time(Duration::from_secs(3));

    let runtime = setup_runtime();

    for &(total_groups, groups_with_messages, expected_synced) in SYNC_SCENARIOS.iter() {
        benchmark_group.throughput(Throughput::Elements(expected_synced as u64));

        benchmark_group.bench_function(
            BenchmarkId::new(
                "sync_all_conversations",
                format!(
                    "{}_groups_{}_with_messages",
                    total_groups, groups_with_messages
                ),
            ),
            |b| {
                let span = trace_span!(BENCH_ROOT_SPAN, total_groups, groups_with_messages);

                b.to_async(&runtime).iter_batched(
                    || {
                        // Setup fresh state for each iteration using bench_async_setup
                        bench_async_setup(|| async {
                            setup_sync_conversations_bench(
                                total_groups,
                                groups_with_messages,
                                expected_synced,
                            )
                            .await
                        })
                    },
                    |setup| {
                        async move {
                            // Get all groups for syncing
                            let groups_to_sync = setup.all_groups.clone();

                            // Call sync_all_groups and measure performance
                            let synced_count =
                                setup.client.sync_all_groups(groups_to_sync).await.unwrap();

                            // Verify sync completed successfully
                            black_box(synced_count);
                        }
                        .instrument(span.clone())
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    benchmark_group.finish();
}

criterion_group!(
    name = sync_conversations;
    config = Criterion::default()
        .sample_size(SAMPLE_SIZE)
        .measurement_time(Duration::from_secs(60))
        .warm_up_time(Duration::from_secs(5));
    targets = bench_sync_all_conversations
);
criterion_main!(sync_conversations);
