//! Identity Generation

use crate::builder::ClientBuilder;
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_id::associations::{test_utils::WalletTestExt, PublicIdentifier};

use super::{BenchClient, BenchError};

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
    pub identifier: PublicIdentifier,
}

impl Identity {
    pub fn new(inbox_id: String, identifier: PublicIdentifier) -> Self {
        Identity {
            inbox_id,
            identifier,
        }
    }
}

async fn create_identity(is_dev_network: bool) -> Identity {
    let wallet = generate_local_wallet();
    let ident = wallet.public_identifier();
    let client = if is_dev_network {
        ClientBuilder::new_dev_client(&wallet).await
    } else {
        ClientBuilder::new_local_client(&wallet).await
    };
    Identity::new(client.inbox_id().to_string(), ident)
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
    client: &BenchClient,
    is_dev_network: bool,
) -> Vec<Identity> {
    let _ = fdlimit::raise_fd_limit();
    match load_identities(is_dev_network) {
        Ok(identities) => {
            tracing::info!(
                "Found generated identities at {}, checking for existence on backend...",
                file_path(is_dev_network)
            );
            if client.is_registered(&identities[0].identifier).await {
                return identities;
            }
        }
        Err(BenchError::Serde(e)) => {
            panic!("{}", e.to_string());
        }
        _ => (),
    }

    tracing::info!(
        "Could not find any identitites to load, creating new identitites \n
        Beware, this fills $TMPDIR with ~10GBs of identities"
    );

    println!("Writing {identities} identities... (this will take a while...)");
    let addresses = write_identities(identities, is_dev_network).await;
    println!("Wrote {identities} to {}", file_path(is_dev_network));
    addresses
}
