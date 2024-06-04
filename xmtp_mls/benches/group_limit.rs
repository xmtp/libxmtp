use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use ethers::signers::LocalWallet;
use std::{collections::HashMap, sync::Arc, sync::Once};
use tokio::runtime::{Builder, Handle, Runtime};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Registry};
use xmtp_cryptography::utils::rng;
use xmtp_mls::{
    builder::ClientBuilder,
    utils::{
        bench::{create_identities_if_dont_exist, Identity},
        test::TestClient,
    },
};

static INIT: Once = Once::new();
pub const IDENTITY_SAMPLES: [usize; 12] = [5, 10, 20, 40, 80, 100, 200, 400, 500, 600, 700, 800];
pub const MAX_IDENTITIES: usize = 5_000;
pub const SAMPLE_SIZE: usize = 10;

pub(crate) fn init_logging() {
    INIT.call_once(|| {
        let fmt = fmt::layer().compact();
        Registry::default()
            .with(EnvFilter::from_default_env())
            .with(fmt)
            .init()
    })
}

fn setup() -> (Arc<TestClient>, Vec<Identity>, Runtime) {
    let runtime = Builder::new_multi_thread()
        .worker_threads(4)
        .enable_time()
        .enable_io()
        .thread_name("xmtp-bencher")
        .build()
        .unwrap();

    let (client, identities) = runtime.block_on(async {
        let identities: Vec<Identity> = create_identities_if_dont_exist(MAX_IDENTITIES).await;

        let wallet = LocalWallet::new(&mut rng());
        let client = Arc::new(ClientBuilder::new_test_client(&wallet).await);

        (client, identities)
    });

    (client, identities, runtime)
}

/// criterion `batch_iter` surrounds the closure in a `Runtime.block_on` despite being a sync
/// function, even in the async 'to_async` setup. Therefore we do this (only _slightly_) hacky
/// workaround to allow us to async setup some groups.
fn bench_async_setup<F, T, Fut>(fun: F) -> T
where
    F: Fn() -> Fut,
    Fut: futures::future::Future<Output = T>,
{
    tokio::task::block_in_place(move || Handle::current().block_on(async move { fun().await }))
}

fn add_to_empty_group(c: &mut Criterion) {
    init_logging();
    let mut benchmark_group = c.benchmark_group("add_to_empty_group");
    benchmark_group.sample_size(SAMPLE_SIZE);

    let (client, identities, runtime) = setup();
    let addresses: Vec<String> = identities.into_iter().map(|i| i.address).collect();

    let mut map = HashMap::<usize, Vec<String>>::new();
    for size in IDENTITY_SAMPLES {
        map.insert(size, addresses.iter().take(size).cloned().collect());
    }

    for size in IDENTITY_SAMPLES.iter() {
        benchmark_group.throughput(Throughput::Elements(*size as u64));
        benchmark_group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let addrs = map.get(&size).unwrap();
            b.to_async(&runtime).iter_batched(
                || (client.clone(), client.create_group(None).unwrap()),
                |(client, group)| async move {
                    group.add_members(&client, addrs.clone()).await.unwrap();
                },
                BatchSize::SmallInput,
            );
        });
    }
    benchmark_group.finish();
}

fn add_to_empty_group_by_inbox_id(c: &mut Criterion) {
    init_logging();
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
            b.to_async(&runtime).iter_batched(
                || (client.clone(), client.create_group(None).unwrap()),
                |(client, group)| async move {
                    group
                        .add_members_by_inbox_id(&client, ids.clone())
                        .await
                        .unwrap();
                },
                BatchSize::SmallInput,
            );
        });
    }
    benchmark_group.finish();
}

#[allow(dead_code)]
// requires https://github.com/xmtp/libxmtp/issues/810 to work
fn add_to_100_member_group_by_inbox_id(c: &mut Criterion) {
    init_logging();
    let mut benchmark_group = c.benchmark_group("add_to_100_member_group_by_inbox_id");
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
            // let setup = setup(&runtime);
            b.to_async(&runtime).iter_batched(
                || {
                    bench_async_setup(|| async {
                        let group = client.create_group(None).unwrap();
                        group
                            .add_members_by_inbox_id(
                                &client,
                                // it is OK to take from the back for now because we aren't getting
                                // near MAX_IDENTITIES
                                inbox_ids.iter().rev().take(100).cloned().collect(),
                            )
                            .await
                            .unwrap();

                        (client.clone(), group)
                    })
                },
                |(client, group)| {
                    let client = client.clone();
                    async move {
                        group
                            .add_members_by_inbox_id(&client, ids.clone())
                            .await
                            .unwrap();
                    }
                },
                BatchSize::SmallInput,
            );
        });
    }
    benchmark_group.finish();
}

#[allow(dead_code)]
// requires https://github.com/xmtp/libxmtp/issues/810 to work
fn add_to_100_member_group_by_address(c: &mut Criterion) {
    init_logging();
    let mut benchmark_group = c.benchmark_group("add_to_100_member_group_by_address");
    benchmark_group.sample_size(SAMPLE_SIZE);

    let (client, identities, runtime) = setup();
    let addresses: Vec<String> = identities.into_iter().map(|i| i.address).collect();

    let mut map = HashMap::<usize, Vec<String>>::new();

    for size in IDENTITY_SAMPLES {
        map.insert(size, addresses.iter().take(size).cloned().collect());
    }

    for size in IDENTITY_SAMPLES.iter() {
        benchmark_group.throughput(Throughput::Elements(*size as u64));
        benchmark_group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let ids = map.get(&size).unwrap();
            b.to_async(&runtime).iter_batched(
                || {
                    bench_async_setup(|| async {
                        let group = client.create_group(None).unwrap();
                        group
                            .add_members(
                                &client,
                                // it is OK to take from the back for now because we aren't getting
                                // near MAX_IDENTITIES
                                addresses.iter().rev().take(100).cloned().collect(),
                            )
                            .await
                            .unwrap();

                        (client.clone(), group)
                    })
                },
                |(client, group)| async move {
                    println!("Adding {} to group", ids.len());
                    group.add_members(&client, ids.clone()).await.unwrap();
                },
                BatchSize::SmallInput,
            );
        });
    }
    benchmark_group.finish();
}

fn remove_all_members_from_group(c: &mut Criterion) {
    init_logging();
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

            b.to_async(&runtime).iter_batched(
                || {
                    bench_async_setup(|| async {
                        let group = client.create_group(None).unwrap();
                        group
                            .add_members_by_inbox_id(&client, ids.clone())
                            .await
                            .unwrap();
                        (client.clone(), group)
                    })
                },
                |(client, group)| async move {
                    group
                        .remove_members_by_inbox_id(&client, ids.clone())
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
    init_logging();
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

            b.to_async(&runtime).iter_batched(
                || {
                    bench_async_setup(|| async {
                        let group = client.create_group(None).unwrap();
                        group
                            .add_members_by_inbox_id(&client, ids.clone())
                            .await
                            .unwrap();
                        (client.clone(), group)
                    })
                },
                |(client, group)| async move {
                    group
                        .remove_members_by_inbox_id(&client, ids[0..(size / 2)].into())
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
    targets = add_to_empty_group, add_to_empty_group_by_inbox_id, remove_all_members_from_group, remove_half_members_from_group);
criterion_main!(group_limit);
