//! Benchmarks for message finding operations with shared setup
//!
//! This version shares the expensive setup between benchmark samples to reduce total runtime.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::{hint::black_box, sync::Arc, time::Duration};
use tokio::runtime::{Builder, Runtime};
use tracing::{Instrument, trace_span};
use xmtp_common::bench::{self, BENCH_ROOT_SPAN};
use xmtp_db::encrypted_store::group_message::{
    ContentType, DeliveryStatus, GroupMessageKind, MsgQueryArgs, SortDirection,
};
use xmtp_mls::utils::bench::{MessageBenchSetup, new_client, setup_group_with_messages};

pub const MESSAGE_COUNTS: [usize; 5] = [10, 100, 1000, 10000, 50000];
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

fn bench_find_messages(c: &mut Criterion) {
    bench::logger();
    let mut benchmark_group = c.benchmark_group("find_messages_shared");
    benchmark_group.sample_size(SAMPLE_SIZE);
    benchmark_group.measurement_time(Duration::from_secs(30));
    benchmark_group.warm_up_time(Duration::from_secs(3));

    let runtime = Arc::new(setup_runtime());

    for &total_messages in MESSAGE_COUNTS.iter() {
        benchmark_group.throughput(Throughput::Elements(10_u64)); // Limit of 10

        // Setup once per MESSAGE_COUNT - completely outside the benchmark
        let setup = runtime.block_on(setup_benchmark(total_messages));
        let runtime_clone = runtime.clone();

        benchmark_group.bench_function(
            BenchmarkId::new("find_messages_limit_10", total_messages),
            move |b| {
                let span = trace_span!(BENCH_ROOT_SPAN, total_messages);
                let setup = setup.clone();
                let runtime = runtime_clone.clone();

                b.iter(|| {
                    runtime.block_on(
                        async {
                            let messages = setup
                                .group
                                .find_messages(&MsgQueryArgs {
                                    limit: Some(10),
                                    ..Default::default()
                                })
                                .unwrap();

                            assert_eq!(
                                messages.len(),
                                10,
                                "Expected exactly 10 messages, got {}",
                                messages.len()
                            );
                            black_box(messages);
                        }
                        .instrument(span.clone()),
                    )
                });
            },
        );
    }

    benchmark_group.finish();
}

fn bench_find_messages_v2(c: &mut Criterion) {
    bench::logger();
    let mut benchmark_group = c.benchmark_group("find_messages_v2_shared");
    benchmark_group.sample_size(SAMPLE_SIZE);
    benchmark_group.measurement_time(Duration::from_secs(30));
    benchmark_group.warm_up_time(Duration::from_secs(3));

    let runtime = Arc::new(setup_runtime());

    for &total_messages in MESSAGE_COUNTS.iter() {
        benchmark_group.throughput(Throughput::Elements(10_u64)); // Limit of 10

        // Setup once per MESSAGE_COUNT - completely outside the benchmark
        let setup = runtime.block_on(setup_benchmark(total_messages));
        let runtime_clone = runtime.clone();

        benchmark_group.bench_function(
            BenchmarkId::new("find_messages_v2_limit_10", total_messages),
            move |b| {
                let span = trace_span!(BENCH_ROOT_SPAN, total_messages);
                let setup = setup.clone();
                let runtime = runtime_clone.clone();

                b.iter(|| {
                    runtime.block_on(
                        async {
                            let messages = setup
                                .group
                                .find_messages_v2(&MsgQueryArgs {
                                    limit: Some(10),
                                    ..Default::default()
                                })
                                .unwrap();

                            assert_eq!(
                                messages.len(),
                                10,
                                "Expected exactly 10 messages from find_messages_v2, got {}",
                                messages.len()
                            );
                            black_box(messages);
                        }
                        .instrument(span.clone()),
                    )
                });
            },
        );
    }

    benchmark_group.finish();
}

fn bench_find_messages_with_time_filters(c: &mut Criterion) {
    bench::logger();
    let mut benchmark_group = c.benchmark_group("find_messages_time_filters_shared");
    benchmark_group.sample_size(SAMPLE_SIZE);
    benchmark_group.measurement_time(Duration::from_secs(30));
    benchmark_group.warm_up_time(Duration::from_secs(3));

    let runtime = Arc::new(setup_runtime());

    for &total_messages in MESSAGE_COUNTS.iter() {
        benchmark_group.throughput(Throughput::Elements(10_u64)); // Limit of 10

        // Setup once per MESSAGE_COUNT - completely outside the benchmark
        let setup = runtime.block_on(setup_benchmark(total_messages));
        let runtime_clone = runtime.clone();

        // Calculate time filter values based on actual message timestamps
        // sent_after_ns: earliest timestamp - 1 to include all messages
        // sent_before_ns: latest timestamp + 1 to include all messages
        let sent_after_ns = setup.earliest_message_timestamp - 1;
        let sent_before_ns = setup.latest_message_timestamp + 1;

        // Benchmark with sent_after_ns filter
        {
            let setup_clone = setup.clone();
            let runtime_clone = runtime_clone.clone();
            benchmark_group.bench_function(
                BenchmarkId::new("find_messages_sent_after", total_messages),
                move |b| {
                    let span = trace_span!(BENCH_ROOT_SPAN, total_messages);
                    let setup = setup_clone.clone();
                    let runtime = runtime_clone.clone();

                    b.iter(|| {
                        runtime.block_on(
                            async {
                                let messages = setup
                                    .group
                                    .find_messages(&MsgQueryArgs {
                                        limit: Some(10),
                                        sent_after_ns: Some(sent_after_ns),
                                        ..Default::default()
                                    })
                                    .unwrap();

                                assert_eq!(messages.len(), 10, "Expected exactly 10 messages with sent_after_ns filter, got {}", messages.len());
                                black_box(messages);
                            }
                            .instrument(span.clone()),
                        )
                    });
                },
            );
        }

        // Benchmark with sent_before_ns filter
        {
            let setup_clone = setup.clone();
            let runtime_clone = runtime_clone.clone();
            benchmark_group.bench_function(
                BenchmarkId::new("find_messages_sent_before", total_messages),
                move |b| {
                    let span = trace_span!(BENCH_ROOT_SPAN, total_messages);
                    let setup = setup_clone.clone();
                    let runtime = runtime_clone.clone();

                    b.iter(|| {
                        runtime.block_on(
                            async {
                                let messages = setup
                                    .group
                                    .find_messages(&MsgQueryArgs {
                                        limit: Some(10),
                                        sent_before_ns: Some(sent_before_ns),
                                        ..Default::default()
                                    })
                                    .unwrap();

                                assert_eq!(messages.len(), 10, "Expected exactly 10 messages with sent_before_ns filter, got {}", messages.len());
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

fn bench_find_messages_with_other_filters(c: &mut Criterion) {
    bench::logger();
    let mut benchmark_group = c.benchmark_group("find_messages_other_filters_shared");
    benchmark_group.sample_size(SAMPLE_SIZE);
    benchmark_group.measurement_time(Duration::from_secs(30));
    benchmark_group.warm_up_time(Duration::from_secs(3));

    let runtime = Arc::new(setup_runtime());

    for &total_messages in MESSAGE_COUNTS.iter() {
        benchmark_group.throughput(Throughput::Elements(10_u64)); // Limit of 10

        // Setup once per MESSAGE_COUNT - completely outside the benchmark
        let setup = runtime.block_on(setup_benchmark(total_messages));
        let runtime_clone = runtime.clone();

        // Benchmark with kind filter
        {
            let setup_clone = setup.clone();
            let runtime_clone = runtime_clone.clone();
            benchmark_group.bench_function(
                BenchmarkId::new("find_messages_kind_application", total_messages),
                move |b| {
                    let span = trace_span!(BENCH_ROOT_SPAN, total_messages);
                    let setup = setup_clone.clone();
                    let runtime = runtime_clone.clone();

                    b.iter(|| {
                        runtime.block_on(
                            async {
                                let messages = setup
                                    .group
                                    .find_messages(&MsgQueryArgs {
                                        limit: Some(10),
                                        kind: Some(GroupMessageKind::Application),
                                        ..Default::default()
                                    })
                                    .unwrap();

                                assert_eq!(
                                    messages.len(),
                                    10,
                                    "Expected exactly 10 messages with kind filter, got {}",
                                    messages.len()
                                );
                                black_box(messages);
                            }
                            .instrument(span.clone()),
                        )
                    });
                },
            );
        }

        // Benchmark with delivery_status filter
        {
            let setup_clone = setup.clone();
            let runtime_clone = runtime_clone.clone();
            benchmark_group.bench_function(
                BenchmarkId::new("find_messages_delivery_unpublished", total_messages),
                move |b| {
                    let span = trace_span!(BENCH_ROOT_SPAN, total_messages);
                    let setup = setup_clone.clone();
                    let runtime = runtime_clone.clone();

                    b.iter(|| {
                        runtime.block_on(
                            async {
                                let messages = setup
                                    .group
                                    .find_messages(&MsgQueryArgs {
                                        limit: Some(10),
                                        delivery_status: Some(DeliveryStatus::Unpublished),
                                        ..Default::default()
                                    })
                                    .unwrap();

                                assert_eq!(messages.len(), 10, "Expected exactly 10 messages with delivery_status filter, got {}", messages.len());
                                black_box(messages);
                            }
                            .instrument(span.clone()),
                        )
                    });
                },
            );
        }

        // Benchmark with content_types filter
        {
            let setup_clone = setup.clone();
            let runtime_clone = runtime_clone.clone();
            benchmark_group.bench_function(
                BenchmarkId::new("find_messages_content_type_text", total_messages),
                move |b| {
                    let span = trace_span!(BENCH_ROOT_SPAN, total_messages);
                    let setup = setup_clone.clone();
                    let runtime = runtime_clone.clone();

                    b.iter(|| {
                        runtime.block_on(
                            async {
                                let messages = setup
                                    .group
                                    .find_messages(&MsgQueryArgs {
                                        limit: Some(10),
                                        content_types: Some(vec![ContentType::Text]),
                                        ..Default::default()
                                    })
                                    .unwrap();

                                assert_eq!(
                                    messages.len(),
                                    10,
                                    "Expected exactly 10 messages with direction filter, got {}",
                                    messages.len()
                                );
                                black_box(messages);
                            }
                            .instrument(span.clone()),
                        )
                    });
                },
            );
        }

        // Benchmark with direction filterin
        {
            let setup_clone = setup.clone();
            let runtime_clone = runtime_clone.clone();
            benchmark_group.bench_function(
                BenchmarkId::new("find_messages_direction_descending", total_messages),
                move |b| {
                    let span = trace_span!(BENCH_ROOT_SPAN, total_messages);
                    let setup = setup_clone.clone();
                    let runtime = runtime_clone.clone();

                    b.iter(|| {
                        runtime.block_on(
                            async {
                                let messages = setup
                                    .group
                                    .find_messages(&MsgQueryArgs {
                                        limit: Some(10),
                                        direction: Some(SortDirection::Descending),
                                        ..Default::default()
                                    })
                                    .unwrap();

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

fn bench_find_messages_v2_with_filters(c: &mut Criterion) {
    bench::logger();
    let mut benchmark_group = c.benchmark_group("find_messages_v2_filters_shared");
    benchmark_group.sample_size(SAMPLE_SIZE);
    benchmark_group.measurement_time(Duration::from_secs(30));
    benchmark_group.warm_up_time(Duration::from_secs(3));

    let runtime = Arc::new(setup_runtime());

    for &total_messages in MESSAGE_COUNTS.iter() {
        benchmark_group.throughput(Throughput::Elements(10_u64)); // Limit of 10

        // Setup once per MESSAGE_COUNT - completely outside the benchmark
        let setup = runtime.block_on(setup_benchmark(total_messages));
        let runtime_clone = runtime.clone();

        // Calculate time filter values based on actual message timestamps
        // sent_after_ns: earliest timestamp - 1 to include all messages
        let sent_after_ns = setup.earliest_message_timestamp - 1;

        // Benchmark find_messages_v2 with sent_after_ns filter
        {
            let setup_clone = setup.clone();
            let runtime_clone = runtime_clone.clone();
            benchmark_group.bench_function(
                BenchmarkId::new("find_messages_v2_sent_after", total_messages),
                move |b| {
                    let span = trace_span!(BENCH_ROOT_SPAN, total_messages);
                    let setup = setup_clone.clone();
                    let runtime = runtime_clone.clone();

                    b.iter(|| {
                        runtime.block_on(
                            async {
                                let messages = setup
                                    .group
                                    .find_messages_v2(&MsgQueryArgs {
                                        limit: Some(10),
                                        sent_after_ns: Some(sent_after_ns),
                                        ..Default::default()
                                    })
                                    .unwrap();

                                assert_eq!(messages.len(), 10, "Expected exactly 10 messages from find_messages_v2 with sent_after_ns filter, got {}", messages.len());
                                black_box(messages);
                            }
                            .instrument(span.clone()),
                        )
                    });
                },
            );
        }

        // Benchmark find_messages_v2 with kind filter
        {
            let setup_clone = setup.clone();
            let runtime_clone = runtime_clone.clone();
            benchmark_group.bench_function(
                BenchmarkId::new("find_messages_v2_kind_application", total_messages),
                move |b| {
                    let span = trace_span!(BENCH_ROOT_SPAN, total_messages);
                    let setup = setup_clone.clone();
                    let runtime = runtime_clone.clone();

                    b.iter(|| {
                        runtime.block_on(
                            async {
                                let messages = setup
                                    .group
                                    .find_messages_v2(&MsgQueryArgs {
                                        limit: Some(10),
                                        kind: Some(GroupMessageKind::Application),
                                        ..Default::default()
                                    })
                                    .unwrap();

                                assert_eq!(messages.len(), 10, "Expected exactly 10 messages from find_messages_v2 with kind filter, got {}", messages.len());
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
             bench_find_messages_v2,
             bench_find_messages_with_time_filters,
             bench_find_messages_with_other_filters,
             bench_find_messages_v2_with_filters
);
criterion_main!(messages);
