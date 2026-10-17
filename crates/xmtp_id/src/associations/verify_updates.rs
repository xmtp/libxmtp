use super::{IdentityUpdate, SignatureError, unverified::UnverifiedIdentityUpdate};
use crate::scw_verifier::SmartContractSignatureVerifier;
use futures::future::try_join_all;

/// Convert a list of unverified updates to verified updates using the given smart contract verifier
pub async fn verify_updates(
    updates: Vec<UnverifiedIdentityUpdate>,
    scw_verifier: impl SmartContractSignatureVerifier,
) -> Result<Vec<IdentityUpdate>, SignatureError> {
    try_join_all(
        updates
            .iter()
            .map(|update| update.to_verified(&scw_verifier)),
    )
    .await
}
