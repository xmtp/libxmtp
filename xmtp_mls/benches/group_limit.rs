#![recursion_limit = "256"]
//! Benchmarks for group limit
//! using `RUST_LOG=trace` will additionally output a `tracing.folded` file, which
//! may be used to generate a flamegraph of execution from tracing logs.
use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use std::{collections::HashMap, sync::Arc};
use tokio::runtime::{Builder, Runtime};
use tracing::{trace_span, Instrument};
use xmtp_common::bench::{self, bench_async_setup, BENCH_ROOT_SPAN};
use xmtp_mls::{
    builder::ClientBuilder,
    groups::GroupMetadataOptions,
    utils::bench::{create_identities_if_dont_exist, BenchClient, Identity},
};

pub const IDENTITY_SAMPLES: [usize; 9] = [10, 20, 40, 80, 100, 200, 300, 400, 450];
pub const MAX_IDENTITIES: usize = 1_000;
pub const SAMPLE_SIZE: usize = 10;

fn setup() -> (Arc<BenchClient>, Vec<Identity>, Runtime) {
    let runtime = Builder::new_multi_thread()
        .enable_time()
        .enable_io()
        .thread_name("xmtp-bencher")
        .build()
        .unwrap();

    let (client, identities) = runtime.block_on(async {
        let wallet = xmtp_cryptography::utils::generate_local_wallet();

        // use dev network if `DEV_GRPC` is set
        let dev = std::env::var("DEV_GRPC");
        let is_dev_network = matches!(dev, Ok(d) if d == "true" || d == "1");
        let client = if is_dev_network {
            tracing::info!("Using Dev GRPC");
            Arc::new(ClientBuilder::new_dev_client(&wallet).await)
        } else {
            Arc::new(ClientBuilder::new_local_client(&wallet).await)
        };

        let identities: Vec<Identity> =
            create_identities_if_dont_exist(MAX_IDENTITIES, client.as_ref(), is_dev_network).await;

        (client, identities)
    });

    (client, identities, runtime)
}

fn add_to_empty_group(c: &mut Criterion) {
    bench::logger();
    let mut benchmark_group = c.benchmark_group("add_to_empty_group");
    benchmark_group.sample_size(SAMPLE_SIZE);

    let (client, identities, runtime) = setup();
    let identifiers: Vec<_> = identities.into_iter().map(|i| i.identifier).collect();

    let mut map = HashMap::<usize, Vec<_>>::new();
    for size in IDENTITY_SAMPLES {
        map.insert(size, identifiers.iter().take(size).cloned().collect());
    }

    for size in IDENTITY_SAMPLES.iter() {
        benchmark_group.throughput(Throughput::Elements(*size as u64));
        benchmark_group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let addrs = map.get(&size).unwrap();
            let span = trace_span!(BENCH_ROOT_SPAN, size);
            b.to_async(&runtime).iter_batched(
                || {
                    (
                        client
                            .create_group(None, GroupMetadataOptions::default())
                            .unwrap(),
                        addrs.clone(),
                        span.clone(),
                    )
                },
                |(group, addrs, span)| async move {
                    group.add_members(&addrs).instrument(span).await.unwrap();
                },
                BatchSize::SmallInput,
            );
        });
    }
    benchmark_group.finish();
}

fn add_to_empty_group_by_inbox_id(c: &mut Criterion) {
    bench::logger();
    let mut benchmark_group = c.benchmark_group("add_to_empty_group_by_inbox_id");
    benchmark_group.sample_size(SAMPLE_SIZE);

    let (client, identities, runtime) = setup();
    let inbox_ids: Vec<String> = identities.into_iter().map(|i| i.inbox_id).collect();

    let mut map = HashMap::<usize, Vec<String>>::new();
    for size in IDENTITY_SAMPLES {
        map.insert(size, inbox_ids.iter().take(size).cloned().collect());
    }

    for size in IDENTITY_SAMPLES.iter() {
        benchmark_group.throughput(Throughput::Elements(*size as u64));
        benchmark_group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let ids = map.get(&size).unwrap();
            let span = trace_span!(BENCH_ROOT_SPAN, size);
            b.to_async(&runtime).iter_batched(
                || {
                    (
                        client
                            .create_group(None, GroupMetadataOptions::default())
                            .unwrap(),
                        span.clone(),
                        ids.clone(),
                    )
                },
                |(group, span, ids)| async move {
                    group
                        .add_members_by_inbox_id(&ids)
                        .instrument(span)
                        .await
                        .unwrap();
                },
                BatchSize::SmallInput,
            );
        });
    }
    benchmark_group.finish();
}

fn add_to_100_member_group_by_inbox_id(c: &mut Criterion) {
    bench::logger();
    let mut benchmark_group = c.benchmark_group("add_to_100_member_group_by_inbox_id");
    benchmark_group.sample_size(SAMPLE_SIZE);

    let (client, identities, runtime) = setup();
    let inbox_ids: Vec<String> = identities
        .into_iter()
        .map(|i| i.inbox_id)
        .rev()
        .take(100)
        .collect();

    let mut map = HashMap::<usize, Vec<String>>::new();

    for size in IDENTITY_SAMPLES {
        map.insert(size, inbox_ids.iter().take(size).cloned().collect());
    }

    for size in IDENTITY_SAMPLES.iter() {
        benchmark_group.throughput(Throughput::Elements(*size as u64));
        benchmark_group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let ids = map.get(&size).unwrap();
            let span = trace_span!(BENCH_ROOT_SPAN, size);
            let id_slice = ids.iter().map(AsRef::as_ref).collect::<Vec<&str>>();
            b.to_async(&runtime).iter_batched(
                || {
                    bench_async_setup(|| async {
                        let group = client
                            .create_group(None, GroupMetadataOptions::default())
                            .unwrap();
                        group
                            .add_members_by_inbox_id(
                                // it is OK to take from the back for now because we aren't getting
                                // near MAX_IDENTITIES
                                &inbox_ids,
                            )
                            .await
                            .unwrap();
                        let id_slice = id_slice.clone();

                        (group, span.clone(), id_slice)
                    })
                },
                |(group, span, id_slice)| async move {
                    group
                        .add_members_by_inbox_id(&id_slice)
                        .instrument(span)
                        .await
                        .unwrap();
                },
                BatchSize::SmallInput,
            );
        });
    }
    benchmark_group.finish();
}

fn remove_all_members_from_group(c: &mut Criterion) {
    bench::logger();
    let mut benchmark_group = c.benchmark_group("remove_all_members_from_group");
    benchmark_group.sample_size(SAMPLE_SIZE);

    let (client, identities, runtime) = setup();
    let inbox_ids: Vec<String> = identities.into_iter().map(|i| i.inbox_id).collect();

    let mut map = HashMap::<usize, Vec<String>>::new();

    for size in IDENTITY_SAMPLES {
        map.insert(size, inbox_ids.iter().take(size).cloned().collect());
    }

    for size in IDENTITY_SAMPLES.iter() {
        benchmark_group.throughput(Throughput::Elements(*size as u64));
        benchmark_group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let ids = map.get(&size).unwrap();
            let id_slice = ids.iter().map(AsRef::as_ref).collect::<Vec<&str>>();
            let span = trace_span!(BENCH_ROOT_SPAN, size);
            b.to_async(&runtime).iter_batched(
                || {
                    bench_async_setup(|| async {
                        let group = client
                            .create_group(None, GroupMetadataOptions::default())
                            .unwrap();
                        group.add_members_by_inbox_id(ids).await.unwrap();
                        let ids = id_slice.clone();
                        (group, span.clone(), ids)
                    })
                },
                |(group, span, ids)| async move {
                    group
                        .remove_members_by_inbox_id(&ids)
                        .instrument(span)
                        .await
                        .unwrap();
                },
                BatchSize::SmallInput,
            );
        });
    }
    benchmark_group.finish();
}

fn remove_half_members_from_group(c: &mut Criterion) {
    bench::logger();
    let mut benchmark_group = c.benchmark_group("remove_half_members_from_group");
    benchmark_group.sample_size(SAMPLE_SIZE);

    let (client, identities, runtime) = setup();
    let inbox_ids: Vec<String> = identities.into_iter().map(|i| i.inbox_id).collect();

    let mut map = HashMap::<usize, Vec<String>>::new();

    for size in IDENTITY_SAMPLES {
        map.insert(size, inbox_ids.iter().take(size).cloned().collect());
    }

    for size in IDENTITY_SAMPLES.iter() {
        benchmark_group.throughput(Throughput::Elements(*size as u64));
        benchmark_group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let ids = map.get(&size).unwrap();
            let span = trace_span!(BENCH_ROOT_SPAN, size);
            b.to_async(&runtime).iter_batched(
                || {
                    bench_async_setup(|| async {
                        let group = client
                            .create_group(None, GroupMetadataOptions::default())
                            .unwrap();
                        group.add_members_by_inbox_id(ids).await.unwrap();
                        let ids = ids
                            .iter()
                            .map(AsRef::as_ref)
                            .take(size / 2)
                            .collect::<Vec<&str>>();
                        (group, span.clone(), ids)
                    })
                },
                |(group, span, ids)| async move {
                    group
                        .remove_members_by_inbox_id(&ids)
                        .instrument(span)
                        .await
                        .unwrap();
                },
                BatchSize::SmallInput,
            );
        });
    }
    benchmark_group.finish();
}

fn add_1_member_to_group(c: &mut Criterion) {
    bench::logger();
    let mut benchmark_group = c.benchmark_group("add_1_member_to_group");
    benchmark_group.sample_size(SAMPLE_SIZE);

    let (client, identities, runtime) = setup();
    let inbox_ids: Vec<String> = identities.into_iter().map(|i| i.inbox_id).collect();

    let mut map = HashMap::<usize, Vec<String>>::new();

    for size in IDENTITY_SAMPLES {
        map.insert(size, inbox_ids.iter().take(size).cloned().collect());
    }

    for size in IDENTITY_SAMPLES.iter() {
        benchmark_group.throughput(Throughput::Elements(*size as u64));
        benchmark_group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let ids = map.get(&size).unwrap();
            let span = trace_span!(BENCH_ROOT_SPAN, size);
            b.to_async(&runtime).iter_batched(
                || {
                    bench_async_setup(|| async {
                        let group = client
                            .create_group(None, GroupMetadataOptions::default())
                            .unwrap();
                        group.add_members_by_inbox_id(ids).await.unwrap();
                        let member = inbox_ids.last().unwrap().clone();
                        (group, vec![member], span.clone())
                    })
                },
                |(group, member, span)| async move {
                    group
                        .add_members_by_inbox_id(&member)
                        .instrument(span)
                        .await
                        .unwrap();
                },
                BatchSize::SmallInput,
            );
        });
    }
    benchmark_group.finish();
}

criterion_group!(
    name = group_limit;
    config = Criterion::default().sample_size(10);
    targets = add_to_empty_group, add_to_empty_group_by_inbox_id, remove_all_members_from_group, remove_half_members_from_group, add_to_100_member_group_by_inbox_id, add_1_member_to_group);
criterion_main!(group_limit);
