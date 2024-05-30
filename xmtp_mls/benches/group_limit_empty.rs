use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use ethers::signers::LocalWallet;
use std::collections::HashMap;
use tokio::runtime::{Builder, Runtime};
use xmtp_cryptography::utils::rng;
use xmtp_mls::{
    builder::ClientBuilder,
    utils::{
        bench::{create_identities_if_dont_exist, IDENTITIES},
        test::TestClient,
    },
};

fn setup() -> (TestClient, Vec<String>, Runtime) {
    let runtime = Builder::new_multi_thread()
        .worker_threads(4)
        .enable_time()
        .enable_io()
        .thread_name("xmtp-bencher")
        .build()
        .unwrap();

    let (client, addresses) = runtime.block_on(async {
        let addresses: Vec<String> = create_identities_if_dont_exist().await;
        let wallet = LocalWallet::new(&mut rng());
        let client = ClientBuilder::new_test_client(&wallet).await;

        (client, addresses)
    });

    (client, addresses, runtime)
}

fn add_to_empty_group(c: &mut Criterion) {
    tracing_subscriber::fmt::init();

    let mut benchmark_group = c.benchmark_group("add_to_empty_group");
    benchmark_group.sample_size(10);

    let total_identities = &IDENTITIES[0..8];

    println!(
        "Setting up {} identities",
        total_identities.iter().sum::<usize>()
    );
    let (client, addresses, runtime) = setup();
    println!("setup finished");

    let mut addresses = addresses.into_iter();
    let mut map = HashMap::<usize, Vec<String>>::new();

    for size in total_identities {
        map.insert(*size, addresses.by_ref().take(*size).collect());
    }

    for size in total_identities.iter() {
        benchmark_group.throughput(Throughput::Elements(*size as u64));
        benchmark_group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let addrs = map.get(&size).unwrap();
            b.to_async(&runtime).iter(|| async {
                let group = client.create_group(None).unwrap();
                println!("Adding {} members", addrs.len());
                group.add_members(&client, addrs.clone()).await.unwrap();
            });
        });
    }
    benchmark_group.finish();
}

criterion_group!(
    name = group_limit_empty;
    config = Criterion::default().sample_size(10);
    targets = add_to_empty_group
);
criterion_main!(group_limit_empty);
