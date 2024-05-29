use criterion::{criterion_group, criterion_main, Criterion};
use ethers::{
    signers::{LocalWallet, Signer},
    types::Address,
};
use tokio::runtime::{Builder, Runtime};
use xmtp_cryptography::utils::rng;
use xmtp_mls::{builder::ClientBuilder, groups::MlsGroup, utils::test::TestClient};

async fn create_group() -> Address {
    let wallet = LocalWallet::new(&mut rng());
    let _ = ClientBuilder::new_test_client(&wallet).await;
    wallet.address()
}

async fn create_groups(n: usize) -> Vec<Address> {
    let mut addresses = Vec::with_capacity(n);
    for _ in 0..n {
        addresses.push(create_group().await);
    }
    addresses
}

fn with_groups<F, T>(num_groups: usize, fun: F) -> T
where
    F: FnOnce(&TestClient, &MlsGroup, Vec<String>, Runtime) -> T,
{
    let runtime = Builder::new_multi_thread()
        .worker_threads(4)
        .enable_time()
        .enable_io()
        .thread_name("xmtp-bencher")
        .build()
        .unwrap();

    let (client, group, addresses) = runtime.block_on(async {
        let addresses: Vec<String> = create_groups(num_groups)
            .await
            .into_iter()
            .map(hex::encode)
            .collect();
        let wallet = LocalWallet::new(&mut rng());
        let client = ClientBuilder::new_test_client(&wallet).await;

        let group = client.create_group(None).unwrap();
        (client, group, addresses)
    });

    fun(&client, &group, addresses, runtime)
}

fn add_100_members(c: &mut Criterion) {
    with_groups(100, |client, group, addresses, runtime| {
        c.bench_function("add 100 members", |b| {
            b.to_async(&runtime)
                .iter(|| group.add_members(client, addresses.clone()))
        });
    })
}

fn add_1000_members(c: &mut Criterion) {
    with_groups(1000, |client, group, addresses, runtime| {
        c.bench_function("add 1000 members", |b| {
            b.to_async(&runtime)
                .iter(|| group.add_members(client, addresses.clone()))
        });
    })
}

criterion_group!(group_limit, add_100_members, add_1000_members);
criterion_main!(group_limit);
