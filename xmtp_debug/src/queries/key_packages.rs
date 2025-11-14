use color_eyre::eyre::{self, Result};
use openmls_rust_crypto::RustCrypto;
use xmtp_cryptography::XmtpInstallationCredential;
use xmtp_mls::verified_key_package_v2::VerifiedKeyPackageV2;

use crate::app::Identity;
use crate::args;

pub async fn key_packages(
    network: &args::BackendOpts,
    identities: impl Iterator<Item = &Identity>,
) -> Result<Vec<VerifiedKeyPackageV2>> {
    let keys: Vec<[u8; 32]> = identities
        .map(|i| {
            let cred = XmtpInstallationCredential::from_bytes(&i.installation_key).unwrap();
            *cred.public_bytes()
        })
        .collect();
    let client = network.connect()?;
    tracing::debug!(
        installation_keys = ?keys.iter().map(hex::encode).collect::<Vec<_>>(),
        "fetching key packages"
    );
    let res = client
        .fetch_key_packages(xmtp_proto::xmtp::mls::api::v1::FetchKeyPackagesRequest {
            installation_keys: keys.iter().map(Vec::from).collect(),
        })
        .await?;
    Ok(res
        .key_packages
        .into_iter()
        .map(|kp| {
            xmtp_mls::verified_key_package_v2::VerifiedKeyPackageV2::from_bytes(
                &RustCrypto::default(),
                kp.key_package_tls_serialized.as_slice(),
            )
            .map_err(eyre::Report::from)
        })
        .collect::<Result<Vec<_>>>()?)
}
