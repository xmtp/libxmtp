use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use ethers::signers::LocalWallet;
use std::collections::HashMap;
use std::sync::Once;
use tokio::runtime::{Builder, Runtime};
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

pub(crate) fn init_logging() {
    INIT.call_once(|| {
        let fmt = fmt::layer().compact();
        Registry::default()
            .with(EnvFilter::from_default_env())
            .with(fmt)
            .init()
    })
}

fn setup() -> (TestClient, Vec<Identity>, Runtime) {
    let runtime = Builder::new_multi_thread()
        .worker_threads(4)
        .enable_time()
        .enable_io()
        .thread_name("xmtp-bencher")
        .build()
        .unwrap();

    let (client, identities) = runtime.block_on(async {
        let identities: Vec<Identity> = create_identities_if_dont_exist().await;

        let wallet = LocalWallet::new(&mut rng());
        let client = ClientBuilder::new_test_client(&wallet).await;

        (client, identities)
    });

    (client, identities, runtime)
}

fn add_to_empty_group(c: &mut Criterion) {
    init_logging();
    let mut benchmark_group = c.benchmark_group("add_to_empty_group");
    benchmark_group.sample_size(10);

    let identity_samples = [
        5, 10, 20, 40, 80, 100, 200, /* 400, 800, 1_000, 2_000, 4_000, 8_000, 10_000, 20_000,*/
    ];
    let (client, identities, runtime) = setup();
    let addresses: Vec<String> = identities.into_iter().map(|i| i.address).collect();

    let mut map = HashMap::<usize, Vec<String>>::new();
    for size in identity_samples {
        map.insert(size, addresses.iter().take(size).cloned().collect());
    }

    for size in identity_samples.iter() {
        benchmark_group.throughput(Throughput::Elements(*size as u64));
        benchmark_group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let addrs = map.get(&size).unwrap();
            b.to_async(&runtime).iter(|| async {
                let group = client.create_group(None).unwrap();
                group.add_members(&client, addrs.clone()).await.unwrap();
            });
        });
    }
    benchmark_group.finish();
}

fn add_to_empty_group_by_inbox_id(c: &mut Criterion) {
    init_logging();
    let mut benchmark_group = c.benchmark_group("add_to_empty_group_by_inbox_id");
    benchmark_group.sample_size(10);

    let identity_samples = [
        5, 10, 20, 40, 80, 100, 200, /*400, 800, 1_000, 2_000, 4_000, 8_000, 10_000, 20_000, */
    ];
    let (client, identities, runtime) = setup();
    let inbox_ids: Vec<String> = identities.into_iter().map(|i| i.inbox_id).collect();

    let mut map = HashMap::<usize, Vec<String>>::new();
    for size in identity_samples {
        map.insert(size, inbox_ids.iter().take(size).cloned().collect());
    }

    for size in identity_samples.iter() {
        benchmark_group.throughput(Throughput::Elements(*size as u64));
        benchmark_group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let ids = map.get(&size).unwrap();
            b.to_async(&runtime).iter(|| async {
                let group = client.create_group(None).unwrap();
                group
                    .add_members_by_inbox_id(&client, ids.clone())
                    .await
                    .unwrap();
            });
        });
    }
    benchmark_group.finish();
}

criterion_group!(
    name = group_limit_empty;
    config = Criterion::default().sample_size(10);
    targets = add_to_empty_group, add_to_empty_group_by_inbox_id);
criterion_main!(group_limit_empty);
