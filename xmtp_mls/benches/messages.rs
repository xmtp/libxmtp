//! Benchmarks for message finding operations with shared setup
//!
//! This version shares the expensive setup between benchmark samples to reduce total runtime.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::{hint::black_box, sync::Arc, time::Duration};
use tokio::runtime::{Builder, Runtime};
use tracing::{Instrument, trace_span};
use xmtp_common::bench::{self, BENCH_ROOT_SPAN};
use xmtp_db::encrypted_store::group_message::{
    ContentType, DeliveryStatus, GroupMessageKind, MsgQueryArgs, SortBy, SortDirection,
};
use xmtp_mls::utils::bench::{MessageBenchSetup, new_client, setup_group_with_messages};

pub const MESSAGE_COUNTS: [usize; 4] = [10, 1000, 10000, 50000];
pub const SAMPLE_SIZE: usize = 10;

fn setup_runtime() -> Runtime {
    Builder::new_multi_thread()
        .enable_time()
        .enable_io()
        .thread_name("xmtp-messages-bencher")
        .build()
        .unwrap()
}

/// Shared setup for all benchmarks - creates client and group with messages once per MESSAGE_COUNT
async fn setup_benchmark(total_messages: usize) -> Arc<MessageBenchSetup> {
    let client = new_client(false).await;
    setup_group_with_messages(client, total_messages).await
}

/// Parameters for a single benchmark variation
struct BenchmarkParams {
    name: &'static str,
    query_args: MsgQueryArgs,
    expected_count: Option<usize>, // None = skip count assertion
}

fn bench_find_messages(c: &mut Criterion) {
    bench::logger();
    let mut benchmark_group = c.benchmark_group("find_messages");
    benchmark_group.sample_size(SAMPLE_SIZE);
    benchmark_group.measurement_time(Duration::from_secs(30));
    benchmark_group.warm_up_time(Duration::from_secs(3));
    benchmark_group.throughput(Throughput::Elements(10_u64)); // Limit of 10

    let runtime = Arc::new(setup_runtime());

    for &total_messages in MESSAGE_COUNTS.iter() {
        // Setup once per MESSAGE_COUNT - completely outside the benchmark
        let setup = runtime.block_on(setup_benchmark(total_messages));

        // Calculate time filter values based on actual message timestamps
        let sent_after_ns = setup.earliest_message_timestamp - 1;
        let sent_before_ns = setup.latest_message_timestamp + 1;

        // Define all benchmark variations for find_messages
        let benchmark_params = vec![
            // Basic limit tests with different sort fields
            BenchmarkParams {
                name: "sent_at_asc",
                query_args: MsgQueryArgs {
                    limit: Some(10),
                    sort_by: Some(SortBy::SentAt),
                    ..Default::default()
                },
                expected_count: Some(10),
            },
            BenchmarkParams {
                name: "sent_at_desc",
                query_args: MsgQueryArgs {
                    limit: Some(10),
                    sort_by: Some(SortBy::SentAt),
                    direction: Some(SortDirection::Descending),
                    ..Default::default()
                },
                expected_count: Some(10),
            },
            BenchmarkParams {
                name: "inserted_at_asc",
                query_args: MsgQueryArgs {
                    limit: Some(10),
                    sort_by: Some(SortBy::InsertedAt),
                    ..Default::default()
                },
                expected_count: Some(10),
            },
            BenchmarkParams {
                name: "inserted_at_desc",
                query_args: MsgQueryArgs {
                    limit: Some(10),
                    sort_by: Some(SortBy::InsertedAt),
                    direction: Some(SortDirection::Descending),
                    ..Default::default()
                },
                expected_count: Some(10),
            },
            // Time filter: sent_after_ns with different sort fields
            BenchmarkParams {
                name: "sent_after_sent_at",
                query_args: MsgQueryArgs {
                    limit: Some(10),
                    sent_after_ns: Some(sent_after_ns),
                    sort_by: Some(SortBy::SentAt),
                    ..Default::default()
                },
                expected_count: Some(10),
            },
            BenchmarkParams {
                name: "sent_after_inserted_at",
                query_args: MsgQueryArgs {
                    limit: Some(10),
                    sent_after_ns: Some(sent_after_ns),
                    sort_by: Some(SortBy::InsertedAt),
                    ..Default::default()
                },
                expected_count: Some(10),
            },
            // Time filter: sent_before_ns with different sort fields
            BenchmarkParams {
                name: "sent_before_sent_at",
                query_args: MsgQueryArgs {
                    limit: Some(10),
                    sent_before_ns: Some(sent_before_ns),
                    sort_by: Some(SortBy::SentAt),
                    ..Default::default()
                },
                expected_count: Some(10),
            },
            BenchmarkParams {
                name: "sent_before_inserted_at",
                query_args: MsgQueryArgs {
                    limit: Some(10),
                    sent_before_ns: Some(sent_before_ns),
                    sort_by: Some(SortBy::InsertedAt),
                    ..Default::default()
                },
                expected_count: Some(10),
            },
            BenchmarkParams {
                name: "inserted_after_inserted_at_asc",
                query_args: MsgQueryArgs {
                    limit: Some(10),
                    inserted_after_ns: Some(100),
                    sort_by: Some(SortBy::InsertedAt),
                    ..Default::default()
                },
                expected_count: Some(10),
            },
            BenchmarkParams {
                name: "inserted_before_inserted_at_desc",
                query_args: MsgQueryArgs {
                    limit: Some(10),
                    inserted_before_ns: Some(sent_before_ns),
                    sort_by: Some(SortBy::InsertedAt),
                    direction: Some(SortDirection::Descending),
                    ..Default::default()
                },
                expected_count: Some(10),
            },
            // Kind filter with different sort fields
            BenchmarkParams {
                name: "kind_application_sent_at",
                query_args: MsgQueryArgs {
                    limit: Some(10),
                    kind: Some(GroupMessageKind::Application),
                    sort_by: Some(SortBy::SentAt),
                    ..Default::default()
                },
                expected_count: Some(10),
            },
            BenchmarkParams {
                name: "kind_application_inserted_at",
                query_args: MsgQueryArgs {
                    limit: Some(10),
                    kind: Some(GroupMessageKind::Application),
                    sort_by: Some(SortBy::InsertedAt),
                    ..Default::default()
                },
                expected_count: Some(10),
            },
            // Delivery status filter with different sort fields
            BenchmarkParams {
                name: "delivery_unpublished",
                query_args: MsgQueryArgs {
                    limit: Some(10),
                    delivery_status: Some(DeliveryStatus::Unpublished),
                    sort_by: Some(SortBy::SentAt),
                    ..Default::default()
                },
                expected_count: Some(10),
            },
            // Content type filter with different sort fields
            BenchmarkParams {
                name: "content_type_text_sent_at",
                query_args: MsgQueryArgs {
                    limit: Some(10),
                    content_types: Some(vec![ContentType::Text]),
                    sort_by: Some(SortBy::SentAt),
                    ..Default::default()
                },
                expected_count: Some(10),
            },
            BenchmarkParams {
                name: "content_type_text_inserted_at",
                query_args: MsgQueryArgs {
                    limit: Some(10),
                    content_types: Some(vec![ContentType::Text]),
                    sort_by: Some(SortBy::InsertedAt),
                    ..Default::default()
                },
                expected_count: Some(10),
            },
            BenchmarkParams {
                name: "content_type_no_results",
                query_args: MsgQueryArgs {
                    limit: Some(10),
                    content_types: Some(vec![ContentType::ReadReceipt]),
                    sort_by: Some(SortBy::SentAt),
                    ..Default::default()
                },
                expected_count: Some(0),
            },
            BenchmarkParams {
                name: "exclude_content_types",
                query_args: MsgQueryArgs {
                    limit: Some(10),
                    exclude_content_types: Some(vec![ContentType::Text]),
                    sort_by: Some(SortBy::SentAt),
                    ..Default::default()
                },
                expected_count: Some(0),
            },
            BenchmarkParams {
                name: "exclude_sender_inbox_ids",
                query_args: MsgQueryArgs {
                    limit: Some(10),
                    exclude_sender_inbox_ids: Some(vec!["foo".to_string()]),
                    sort_by: Some(SortBy::SentAt),
                    ..Default::default()
                },
                expected_count: Some(10),
            },
        ];

        // Run benchmarks for each variation
        for params in benchmark_params {
            let setup = setup.clone();
            let runtime_clone = runtime.clone();
            let query_args = params.query_args.clone();
            let expected_count = params.expected_count;

            benchmark_group.bench_with_input(
                BenchmarkId::new(params.name, total_messages),
                &total_messages,
                move |b, &msg_count| {
                    let span = trace_span!(BENCH_ROOT_SPAN, total_messages = msg_count);
                    let setup = setup.clone();
                    let runtime = runtime_clone.clone();
                    let query_args = query_args.clone();

                    b.iter(|| {
                        runtime.block_on(
                            async {
                                let messages = setup.group.find_messages(&query_args).unwrap();

                                if let Some(expected) = expected_count {
                                    assert_eq!(
                                        messages.len(),
                                        expected,
                                        "Expected exactly {} messages, got {}",
                                        expected,
                                        messages.len()
                                    );
                                }
                                black_box(messages);
                            }
                            .instrument(span.clone()),
                        )
                    });
                },
            );
        }
    }

    benchmark_group.finish();
}

fn bench_find_messages_v2(c: &mut Criterion) {
    bench::logger();
    let mut benchmark_group = c.benchmark_group("find_messages_v2");
    benchmark_group.sample_size(SAMPLE_SIZE);
    benchmark_group.measurement_time(Duration::from_secs(30));
    benchmark_group.warm_up_time(Duration::from_secs(3));
    benchmark_group.throughput(Throughput::Elements(10_u64)); // Limit of 10

    let runtime = Arc::new(setup_runtime());

    for &total_messages in MESSAGE_COUNTS.iter() {
        // Setup once per MESSAGE_COUNT - completely outside the benchmark
        let setup = runtime.block_on(setup_benchmark(total_messages));

        // Calculate time filter values based on actual message timestamps
        let sent_after_ns = setup.earliest_message_timestamp - 1;

        // Define all benchmark variations for find_messages_v2
        let benchmark_params = vec![
            // Basic limit test
            BenchmarkParams {
                name: "limit_10",
                query_args: MsgQueryArgs {
                    limit: Some(10),
                    ..Default::default()
                },
                expected_count: Some(10),
            },
            // Time filter: sent_after_ns
            BenchmarkParams {
                name: "sent_after",
                query_args: MsgQueryArgs {
                    limit: Some(10),
                    sent_after_ns: Some(sent_after_ns),
                    ..Default::default()
                },
                expected_count: Some(10),
            },
            // Kind filter
            BenchmarkParams {
                name: "kind_application",
                query_args: MsgQueryArgs {
                    limit: Some(10),
                    kind: Some(GroupMessageKind::Application),
                    ..Default::default()
                },
                expected_count: Some(10),
            },
        ];

        // Run benchmarks for each variation
        for params in benchmark_params {
            let setup = setup.clone();
            let runtime_clone = runtime.clone();
            let query_args = params.query_args.clone();
            let expected_count = params.expected_count;

            benchmark_group.bench_with_input(
                BenchmarkId::new(params.name, total_messages),
                &total_messages,
                move |b, &msg_count| {
                    let span = trace_span!(BENCH_ROOT_SPAN, total_messages = msg_count);
                    let setup = setup.clone();
                    let runtime = runtime_clone.clone();
                    let query_args = query_args.clone();

                    b.iter(|| {
                        runtime.block_on(
                            async {
                                let messages = setup.group.find_messages_v2(&query_args).unwrap();

                                if let Some(expected) = expected_count {
                                    assert_eq!(
                                        messages.len(),
                                        expected,
                                        "Expected exactly {} messages from find_messages_v2, got {}",
                                        expected,
                                        messages.len()
                                    );
                                }
                                black_box(messages);
                            }
                            .instrument(span.clone()),
                        )
                    });
                },
            );
        }
    }

    benchmark_group.finish();
}

criterion_group!(
    name = messages;
    config = Criterion::default()
        .sample_size(SAMPLE_SIZE)
        .measurement_time(Duration::from_secs(30))
        .warm_up_time(Duration::from_secs(3));
    targets = bench_find_messages,
             bench_find_messages_v2
);
criterion_main!(messages);
