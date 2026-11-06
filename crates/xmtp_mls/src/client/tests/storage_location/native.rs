use super::*;
use crate::{Client, InboxOwner, utils::test::identity_setup};
use prost::Message;
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_db::prelude::QueryServerConfiguration;
use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;

mod deployment_record;
mod inbox_lookup;
mod location_validation;

/// Write `data_dir`'s deployment record as one that recorded
/// `identifier` for `backend_url`.
async fn seed_deployment(
    data_dir: &std::path::Path,
    backend_url: &str,
    identifier: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let document = serde_json::json!({
        "version": 1,
        "deployments": { backend_url: identifier },
    });
    tokio::fs::write(
        data_dir.join("deployments.json"),
        serde_json::to_vec(&document)?,
    )
    .await?;
    Ok(())
}

fn builder() -> crate::builder::ClientBuilder<xmtp_api_backend::MockBackendClient, ()> {
    let owner = generate_local_wallet();
    let api = xmtp_api_backend::MockBackendClient::new();
    Client::builder(identity_setup(owner))
        .api_client(api)
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
}
