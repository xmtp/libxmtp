use crate::builder::ClientBuilder;
use ethers::signers::{LocalWallet, Signer};
use indicatif::{ParallelProgressIterator, ProgressStyle};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use serde::{Deserialize, Serialize};
use std::sync::mpsc::channel;
use thiserror::Error;
use thread_local::ThreadLocal;
use tokio::runtime::{Builder, Runtime};
use xmtp_cryptography::utils::rng;

#[derive(Debug, Error)]
pub enum BenchError {
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub const IDENTITY_SAMPLES: [usize; 6] = [
    10, 50, 100, 200, 500, 1_000, /*1_500, 2_000, 3_000, 5_000, 10_000, 20_000, */
];

pub fn file_path() -> String {
    format!("{}/identities.generated", env!("CARGO_MANIFEST_DIR"))
}

pub fn write_identities(num_groups: usize) -> Vec<Identity> {
    let identities: Vec<Identity> = create_identities(num_groups).into_iter().collect();
    let json = serde_json::to_string(&identities).unwrap();

    std::fs::write(file_path(), json).unwrap();

    identities
}

pub fn load_identities() -> Result<Vec<Identity>, BenchError> {
    let identities = std::fs::read(file_path())?;
    Ok(serde_json::from_slice(identities.as_slice())?)
}

#[derive(Serialize, Deserialize)]
pub struct Identity {
    pub inbox_id: String,
    pub address: String,
}

impl Identity {
    pub fn new(inbox_id: String, address: String) -> Self {
        Identity { inbox_id, address }
    }
}

fn create_identity(runtime: &Runtime) -> Identity {
    let wallet = LocalWallet::new(&mut rng());
    let client = runtime.block_on(ClientBuilder::new_test_client(&wallet));

    Identity::new(client.inbox_id(), format!("{:x}", wallet.address()))
}

fn create_identities(n: usize) -> Vec<Identity> {
    let mut addresses = Vec::with_capacity(n);

    let (tx, rx) = channel();

    let style =
        ProgressStyle::with_template("{bar} {pos}/{len} elapsed {elapsed} remaining {eta_precise}");

    let get_runtime = || -> Runtime {
        Builder::new_current_thread()
            .enable_time()
            .enable_io()
            .thread_name("xmtp-identity-gen")
            .build()
            .unwrap()
    };

    let tl = ThreadLocal::new();
    (0..n)
        .collect::<Vec<usize>>()
        .par_iter()
        .progress_count(n as u64)
        .with_style(style.unwrap())
        .for_each(|_| {
            let runtime = tl.get_or(get_runtime);
            tx.send(create_identity(runtime)).unwrap();
        });

    while let Ok(addr) = rx.try_recv() {
        addresses.push(addr);
    }

    addresses
}

pub async fn create_identities_if_dont_exist() -> Vec<Identity> {
    match load_identities() {
        Ok(identities) => {
            println!("Found file");
            let wallet = LocalWallet::new(&mut rng());
            let client = ClientBuilder::new_test_client(&wallet).await;
            if client.is_registered(&identities[0].address).await {
                return identities;
            }
        }
        Err(BenchError::Serde(e)) => {
            panic!("{}", e.to_string());
        }
        _ => (),
    }

    println!(
        "Could not find any identitites to load, creating new identitites \n
        Beware, this fills $TMPDIR with ~10GBs of identities"
    );

    let num_identities = IDENTITY_SAMPLES.iter().sum();
    println!("Writing {num_identities} identities... (this will take a while...)");
    let addresses = write_identities(num_identities);
    println!("Wrote {num_identities} to {}", file_path());
    addresses
}
