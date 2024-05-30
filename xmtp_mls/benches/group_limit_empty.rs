use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use ethers::{
    signers::{LocalWallet, Signer},
    types::Address,
};
use std::collections::HashMap;
use tokio::runtime::{Builder, Runtime};
use xmtp_cryptography::utils::rng;
use xmtp_mls::{builder::ClientBuilder, groups::MlsGroup, utils::test::TestClient};

async fn create_identity() -> Address {
    let wallet = LocalWallet::new(&mut rng());
    let _ = ClientBuilder::new_test_client(&wallet).await;
    wallet.address()
}

async fn create_identities(n: usize) -> Vec<Address> {
    let mut addresses = Vec::with_capacity(n);
    for _ in 0..n {
        addresses.push(create_identity().await);
    }
    addresses
}

fn setup(num_groups: usize) -> (TestClient, MlsGroup, Vec<String>, Runtime) {
    let runtime = Builder::new_multi_thread()
        .worker_threads(4)
        .enable_time()
        .enable_io()
        .thread_name("xmtp-bencher")
        .build()
        .unwrap();

    let (client, group, addresses) = runtime.block_on(async {
        let addresses: Vec<String> = create_identities(num_groups)
            .await
            .into_iter()
            .map(hex::encode)
            .collect();
        let wallet = LocalWallet::new(&mut rng());
        let client = ClientBuilder::new_test_client(&wallet).await;

        let group = client.create_group(None).unwrap();
        (client, group, addresses)
    });

    (client, group, addresses, runtime)
}

fn add_to_empty_group(c: &mut Criterion) {
    tracing_subscriber::fmt::init();

    let mut benchmark_group = c.benchmark_group("add_to_empty_group");
    benchmark_group.sample_size(10);

    let total_identities = [10, 100, 250, 500, 1_000, 2_000]; /* 5_000, 10_000 + 20_000 + 40_000];*/

    println!(
        "Setting up {} identities",
        total_identities.iter().sum::<usize>()
    );
    let (client, group, addresses, runtime) = setup(total_identities.iter().sum());
    println!("setup finished");

    let mut addresses = addresses.into_iter();
    let mut map = HashMap::<usize, Vec<String>>::new();

    for size in total_identities {
        map.insert(size, addresses.by_ref().take(size).collect());
    }

    for size in total_identities.iter() {
        benchmark_group.throughput(Throughput::Elements(*size as u64));
        benchmark_group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let addrs = map.get(&size).unwrap();
            b.to_async(&runtime).iter(|| {
                println!("Adding {} members", addrs.len());
                group.add_members(&client, addrs.clone())
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
