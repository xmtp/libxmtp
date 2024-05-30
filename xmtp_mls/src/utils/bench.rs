use crate::builder::ClientBuilder;
use anyhow::Error;
use ethers::{
    signers::{LocalWallet, Signer},
    types::Address,
};
use indicatif::{ParallelProgressIterator, ProgressStyle};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::sync::mpsc::channel;
use tokio::runtime::Builder;
use xmtp_cryptography::utils::rng;

pub const IDENTITIES: [usize; 14] = [
    10, 50, 100, 200, 500, 1_000, 1_500, 2_000, 3_000, 5_000, 10_000, 20_000, 30_000, 40_000,
];

pub fn file_path() -> String {
    format!("{}/identities.generated", env!("CARGO_MANIFEST_DIR"))
}

pub fn write_identities(num_groups: usize) -> Vec<String> {
    let addresses: Vec<String> = create_identities(num_groups)
        .into_iter()
        .map(hex::encode)
        .collect();
    let json = serde_json::to_string(&addresses).unwrap();

    std::fs::write(file_path(), json).unwrap();

    addresses
}

pub fn load_identities() -> Result<Vec<String>, Error> {
    let addresses = std::fs::read(file_path())?;
    Ok(serde_json::from_slice(addresses.as_slice())?)
}

fn create_identity() -> Address {
    let runtime = Builder::new_current_thread()
        .enable_time()
        .enable_io()
        .thread_name("xmtp-identity-gen")
        .build()
        .unwrap();

    let wallet = LocalWallet::new(&mut rng());
    let _ = runtime.block_on(ClientBuilder::new_test_client(&wallet));
    wallet.address()
}

fn create_identities(n: usize) -> Vec<Address> {
    let mut addresses = Vec::with_capacity(n);

    let (tx, rx) = channel();

    let style =
        ProgressStyle::with_template("{bar} {pos}/{len} elapsed {elapsed} remaining {eta_precise}");
    (0..n)
        .collect::<Vec<usize>>()
        .par_iter()
        .progress_count(n as u64)
        .with_style(style.unwrap())
        .for_each(|_| {
            tx.send(create_identity()).unwrap();
        });

    while let Ok(addr) = rx.try_recv() {
        addresses.push(addr);
    }

    addresses
}

pub async fn create_identities_if_dont_exist() -> Vec<String> {
    println!("Beware, this fills $TMPDIR with ~10GBs of identities");

    if let Ok(identities) = load_identities() {
        let wallet = LocalWallet::new(&mut rng());
        let client = ClientBuilder::new_test_client(&wallet).await;
        if client.is_registered(&identities[0]).await {
            return identities;
        }
    }

    let num_identities = IDENTITIES.iter().sum();
    println!("Writing {num_identities} identities... (this will take a while...)");
    let addresses = write_identities(num_identities);
    println!("Wrote {num_identities} to {}", file_path());
    addresses
}
