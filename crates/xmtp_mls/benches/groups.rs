//! Benchmarks for group finding and listing operations with shared setup
//!
//! This version shares the expensive setup between benchmark samples to reduce total runtime.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::{hint::black_box, sync::Arc, time::Duration};
use tokio::runtime::{Builder, Runtime};
use tracing::{Instrument, trace_span};
use xmtp_common::bench::{self, BENCH_ROOT_SPAN};
use xmtp_db::group::GroupQueryArgs;
use xmtp_mls::utils::bench::{GroupBenchSetup, new_client, setup_groups_with_messages};

pub const GROUP_COUNTS: [usize; 4] = [10, 100, 1000, 10000];
pub const TARGET_GROUPS: usize = 10;
pub const SAMPLE_SIZE: usize = 10;

fn setup_runtime() -> Runtime {
    Builder::new_multi_thread()
        .enable_time()
        .enable_io()
        .thread_name("xmtp-bencher")
        .build()
        .unwrap()
}

/// Shared setup for all benchmarks - creates client and groups once per GROUP_COUNT
async fn setup_benchmark(total_groups: usize) -> Arc<GroupBenchSetup> {
    let client = new_client(false).await;
    let setup = setup_groups_with_messages(client, total_groups, TARGET_GROUPS).await;
    Arc::new(setup)
}

fn bench_find_groups(c: &mut Criterion) {
    bench::logger();
    let mut benchmark_group = c.benchmark_group("find_groups_shared");
    benchmark_group.sample_size(SAMPLE_SIZE);
    // Increase measurement time to handle the expensive setup
    benchmark_group.measurement_time(Duration::from_secs(30));
    benchmark_group.warm_up_time(Duration::from_secs(3));

    let runtime = Arc::new(setup_runtime());

    for &total_groups in GROUP_COUNTS.iter() {
        benchmark_group.throughput(Throughput::Elements(TARGET_GROUPS as u64));

        // Setup once per GROUP_COUNT - completely outside the benchmark
        let setup = runtime.block_on(setup_benchmark(total_groups));
        let runtime_clone = runtime.clone();

        benchmark_group.bench_function(
            BenchmarkId::new("find_10_groups", total_groups),
            move |b| {
                let span = trace_span!(BENCH_ROOT_SPAN, total_groups);
                let setup = setup.clone();
                let runtime = runtime_clone.clone();

                b.iter(|| {
                    runtime.block_on(
                        async {
                            let groups = setup
                                .client
                                .find_groups(GroupQueryArgs {
                                    limit: Some(TARGET_GROUPS as i64),
                                    ..Default::default()
                                })
                                .unwrap();

                            assert!(groups.len() >= TARGET_GROUPS.min(total_groups));
                            black_box(groups);
                        }
                        .instrument(span.clone()),
                    )
                });
            },
        );
    }

    benchmark_group.finish();
}

fn bench_list_conversations(c: &mut Criterion) {
    bench::logger();
    let mut benchmark_group = c.benchmark_group("list_conversations_shared");
    benchmark_group.sample_size(SAMPLE_SIZE);
    // Increase measurement time to handle the expensive setup
    benchmark_group.measurement_time(Duration::from_secs(30));
    benchmark_group.warm_up_time(Duration::from_secs(3));

    let runtime = Arc::new(setup_runtime());

    for &total_groups in GROUP_COUNTS.iter() {
        benchmark_group.throughput(Throughput::Elements(TARGET_GROUPS as u64));

        // Setup once per GROUP_COUNT - completely outside the benchmark
        let setup = runtime.block_on(setup_benchmark(total_groups));
        let runtime_clone = runtime.clone();

        benchmark_group.bench_function(
            BenchmarkId::new("list_10_conversations", total_groups),
            move |b| {
                let span = trace_span!(BENCH_ROOT_SPAN, total_groups);
                let setup = setup.clone();
                let runtime = runtime_clone.clone();

                b.iter(|| {
                    runtime.block_on(
                        async {
                            let conversations = setup
                                .client
                                .list_conversations(GroupQueryArgs {
                                    limit: Some(TARGET_GROUPS as i64),
                                    ..Default::default()
                                })
                                .unwrap();

                            assert!(conversations.len() >= TARGET_GROUPS.min(total_groups));
                            black_box(conversations);
                        }
                        .instrument(span.clone()),
                    )
                });
            },
        );
    }

    benchmark_group.finish();
}

fn bench_find_groups_with_filters(c: &mut Criterion) {
    bench::logger();
    let mut benchmark_group = c.benchmark_group("find_groups_with_filters_shared");
    benchmark_group.sample_size(SAMPLE_SIZE);
    // Increase measurement time to handle the expensive setup
    benchmark_group.measurement_time(Duration::from_secs(30));
    benchmark_group.warm_up_time(Duration::from_secs(3));

    let runtime = Arc::new(setup_runtime());

    for &total_groups in GROUP_COUNTS.iter() {
        benchmark_group.throughput(Throughput::Elements(TARGET_GROUPS as u64));

        // Setup once per GROUP_COUNT - completely outside the benchmark
        let setup = runtime.block_on(setup_benchmark(total_groups));
        let created_after_ns = setup.target_groups[0].created_at_ns - 1000;
        let runtime_clone = runtime.clone();

        benchmark_group.bench_function(
            BenchmarkId::new("find_10_groups_filtered", total_groups),
            move |b| {
                let span = trace_span!(BENCH_ROOT_SPAN, total_groups);
                let setup = setup.clone();
                let runtime = runtime_clone.clone();

                b.iter(|| {
                    runtime.block_on(
                        async {
                            let groups = setup
                                .client
                                .find_groups(GroupQueryArgs {
                                    limit: Some(TARGET_GROUPS as i64),
                                    created_after_ns: Some(created_after_ns),
                                    ..Default::default()
                                })
                                .unwrap();

                            assert!(!groups.is_empty());
                            black_box(groups);
                        }
                        .instrument(span.clone()),
                    )
                });
            },
        );
    }

    benchmark_group.finish();
}

criterion_group!(
    name = groups;
    config = Criterion::default()
        .sample_size(SAMPLE_SIZE)
        .measurement_time(Duration::from_secs(30))
        .warm_up_time(Duration::from_secs(3));
    targets = bench_find_groups, bench_list_conversations, bench_find_groups_with_filters
);
criterion_main!(groups);
