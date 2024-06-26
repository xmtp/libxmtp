//! Utilities for xmtp_mls benchmarks
//! Utilities mostly include pre-generating identities in order to save time when writing/testing
//! benchmarks.
use crate::builder::ClientBuilder;
use ethers::signers::{LocalWallet, Signer};
use indicatif::{ProgressBar, ProgressStyle};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::sync::Once;
use thiserror::Error;
use tracing::{Metadata, Subscriber};
use tracing_flame::{FlameLayer, FlushGuard};
use tracing_subscriber::{
    layer::{Context, Filter, Layer, SubscriberExt},
    registry::LookupSpan,
    util::SubscriberInitExt,
    EnvFilter,
};
use xmtp_cryptography::utils::rng;

use super::test::TestClient;

pub const BENCH_ROOT_SPAN: &str = "xmtp-trace-bench";

/// Re-export of functions in private modules for benchmarks
pub mod re_export {
    pub use crate::hpke::encrypt_welcome;
}

#[derive(Debug, Error)]
pub enum BenchError {
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

static INIT: Once = Once::new();

static LOGGER: OnceCell<FlushGuard<std::io::BufWriter<std::fs::File>>> = OnceCell::new();

/// initializes logging for benchmarks
/// - FMT logging is enabled by passing the normal `RUST_LOG` environment variable options.
/// - Generate a flamegraph from tracing data by passing `XMTP_FLAMEGRAPH=trace`
pub fn init_logging() {
    INIT.call_once(|| {
        let (flame_layer, guard) = FlameLayer::with_file("./tracing.folded").unwrap();
        let flame_layer = flame_layer
            .with_threads_collapsed(true)
            .with_module_path(true);
        // .with_empty_samples(false);

        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().with_filter(EnvFilter::from_default_env()))
            .with(
                flame_layer
                    .with_filter(BenchFilter)
                    .with_filter(EnvFilter::from_env("XMTP_FLAMEGRAPH")),
            )
            .init();

        LOGGER.set(guard).unwrap();
    })
}

/// Filters for only spans where the root span name is "bench"
pub struct BenchFilter;

impl<S> Filter<S> for BenchFilter
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup> + std::fmt::Debug,
    for<'lookup> <S as LookupSpan<'lookup>>::Data: std::fmt::Debug,
{
    fn enabled(&self, meta: &Metadata<'_>, cx: &Context<'_, S>) -> bool {
        if meta.name() == BENCH_ROOT_SPAN {
            return true;
        }
        if let Some(id) = cx.current_span().id() {
            if let Some(s) = cx.span_scope(id) {
                if let Some(s) = s.from_root().take(1).collect::<Vec<_>>().first() {
                    return s.name() == BENCH_ROOT_SPAN;
                }
            }
        }
        false
    }
}

pub fn file_path(is_dev_network: bool) -> String {
    if is_dev_network {
        format!("{}/dev-identities.generated", env!("CARGO_MANIFEST_DIR"))
    } else {
        format!("{}/identities.generated", env!("CARGO_MANIFEST_DIR"))
    }
}

pub async fn write_identities(num_groups: usize, is_dev_network: bool) -> Vec<Identity> {
    let identities: Vec<Identity> = create_identities(num_groups, is_dev_network)
        .await
        .into_iter()
        .collect();
    let json = serde_json::to_string(&identities).unwrap();

    std::fs::write(file_path(is_dev_network), json).unwrap();

    identities
}

pub fn load_identities(is_dev_network: bool) -> Result<Vec<Identity>, BenchError> {
    let identities = std::fs::read(file_path(is_dev_network))?;
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

async fn create_identity(is_dev_network: bool) -> Identity {
    let wallet = LocalWallet::new(&mut rng());
    let client = if is_dev_network {
        ClientBuilder::new_dev_client(&wallet).await
    } else {
        ClientBuilder::new_test_client(&wallet).await
    };
    Identity::new(client.inbox_id(), format!("0x{:x}", wallet.address()))
}

async fn create_identities(n: usize, is_dev_network: bool) -> Vec<Identity> {
    let mut identities = Vec::with_capacity(n);

    let style =
        ProgressStyle::with_template("{bar} {pos}/{len} elapsed {elapsed} remaining {eta_precise}");

    let mut set = tokio::task::JoinSet::new();
    let bar = ProgressBar::new(n as u64).with_style(style.unwrap());
    let mut handles = vec![];

    for _ in 0..n {
        let bar_pointer = bar.clone();
        handles.push(set.spawn(async move {
            let identity = create_identity(is_dev_network).await;
            bar_pointer.inc(1);
            identity
        }));

        // going above 128 we hit "unable to open database errors"
        // This may be related to open file limits
        if set.len() == 128 {
            if let Some(Ok(identity)) = set.join_next().await {
                identities.push(identity);
            }
        }
    }

    while let Some(Ok(identity)) = set.join_next().await {
        identities.push(identity);
    }

    identities
}

/// Create identities if they don't already exist.
/// creates specified `identities` on the
/// gRPC local docker or development node and saves them to a file.
/// `identities.generated`/`dev-identities.generated`. Uses this file for subsequent runs if
/// node still has those identities.
pub async fn create_identities_if_dont_exist(
    identities: usize,
    client: &TestClient,
    is_dev_network: bool,
) -> Vec<Identity> {
    match load_identities(is_dev_network) {
        Ok(identities) => {
            log::info!(
                "Found generated identities at {}, checking for existence on backend...",
                file_path(is_dev_network)
            );
            if client.is_registered(&identities[0].address).await {
                return identities;
            }
        }
        Err(BenchError::Serde(e)) => {
            panic!("{}", e.to_string());
        }
        _ => (),
    }

    log::info!(
        "Could not find any identitites to load, creating new identitites \n
        Beware, this fills $TMPDIR with ~10GBs of identities"
    );

    println!("Writing {identities} identities... (this will take a while...)");
    let addresses = write_identities(identities, is_dev_network).await;
    println!("Wrote {identities} to {}", file_path(is_dev_network));
    addresses
}
