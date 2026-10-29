use crate::db::Projection;
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use tonic::Status;
use xmtp_id::associations::{Handle, Identifier, MemberIdentifier};
use xmtp_id::scw_verifier::{BlockStamp, ChainBlocks, VerifierError};
use xmtp_mls_validation::AssociationValidation;
use xmtp_proto::xmtp::identity::associations::IdentifierKind;

/// Parse and normalize one lookup key.
///
/// Lookup normalization makes equivalent Ethereum and passkey identifiers map
/// to one projection key, and a handle in any ASCII letter case map to its one
/// lowercase spelling. Signed identity fields must never pass through this
/// function because changing their bytes would invalidate their signatures.
/// The error messages are fixed, so no handle reaches a status or a log.
// implements: API-299
pub(crate) fn lookup_key(value: &str, kind: i32) -> Result<(String, i16), Status> {
    let kind = IdentifierKind::try_from(kind)
        .map_err(|_| Status::invalid_argument("unknown identifier kind"))?;
    if kind == IdentifierKind::Handle {
        let handle =
            Handle::from_input(value).map_err(|_| Status::invalid_argument("malformed handle"))?;
        return Ok(handle_key(&handle));
    }
    let identifier = Identifier::from_proto(value, kind, None)
        .map_err(|_| Status::invalid_argument("malformed identifier"))?;
    Ok(identifier_key(&identifier))
}

/// Convert an owned identifier to the database projection key.
///
/// Ethereum addresses use lowercase ASCII; passkey keys use lowercase hex.
fn identifier_key(identifier: &Identifier) -> (String, i16) {
    match identifier {
        Identifier::Ethereum(address) => (
            address.0.to_ascii_lowercase(),
            IdentifierKind::Ethereum as i16,
        ),
        Identifier::Passkey(passkey) => (hex::encode(&passkey.key), IdentifierKind::Passkey as i16),
    }
}

/// The projection key of a handle: its one spelling and the handle kind.
fn handle_key(handle: &Handle) -> (String, i16) {
    (handle.as_str().to_owned(), IdentifierKind::Handle as i16)
}

/// Convert a member identifier to the normalized projection key when supported.
///
/// Invalid or unsupported member forms are omitted. Signed identity data is
/// never normalized here; this helper is only for the derived lookup table.
fn member_key(member: &MemberIdentifier) -> Option<(String, i16)> {
    let identifier: Option<Identifier> = member.clone().into();
    identifier.as_ref().map(identifier_key)
}

/// Derive projection changes from a validated identity transition.
///
/// The result contains only identifiers that changed active status. It compares
/// normalized keys, while also recovering unchanged aliases from the prior state
/// so a projection update cannot remove an association that remains active.
/// Each claimed handle is added, including a handle the inbox claims again.
/// No handle is ever removed: an old handle still resolves to its inbox.
pub(crate) fn projection(validation: &AssociationValidation) -> Projection {
    let active: HashSet<_> = validation
        .state
        .identifiers()
        .iter()
        .map(identifier_key)
        .collect();
    let changed: HashSet<_> = validation
        .diff
        .new_members
        .iter()
        .chain(&validation.diff.removed_members)
        .filter_map(member_key)
        .collect();
    // Recover prior normalized membership to preserve unchanged aliases.
    let new_raw: HashSet<_> = validation.diff.new_members.iter().cloned().collect();
    let prior: HashSet<_> = validation
        .state
        .members()
        .iter()
        .filter(|member| !new_raw.contains(&member.identifier))
        .filter_map(|member| member_key(&member.identifier))
        .chain(
            validation
                .diff
                .removed_members
                .iter()
                .filter_map(member_key),
        )
        .collect();
    Projection {
        added: changed
            .iter()
            .filter(|key| active.contains(*key) && !prior.contains(*key))
            .cloned()
            .chain(validation.claimed_handles.iter().map(handle_key))
            .collect(),
        removed: changed
            .into_iter()
            .filter(|key| !active.contains(key) && prior.contains(key))
            .collect(),
    }
}

/// Chain block reads for one publish: each block's timestamp is read once and
/// reused by every update in the request. Only a successful read is kept. The
/// head is read for each update, so each is judged against a current head.
/// Built per request, so nothing outlives it.
pub(crate) struct PublishChainReads<'a> {
    chain: &'a dyn ChainBlocks,
    timestamps: Mutex<HashMap<(String, u64), u64>>,
}

impl<'a> PublishChainReads<'a> {
    pub(crate) fn new(chain: &'a dyn ChainBlocks) -> Self {
        Self {
            chain,
            timestamps: Mutex::default(),
        }
    }
}

#[xmtp_common::async_trait]
impl ChainBlocks for PublishChainReads<'_> {
    async fn head(&self, chain_id: &str) -> Result<BlockStamp, VerifierError> {
        self.chain.head(chain_id).await
    }

    async fn timestamp(&self, chain_id: &str, number: u64) -> Result<u64, VerifierError> {
        let key = (chain_id.to_owned(), number);
        let kept = self.timestamps.lock().get(&key).copied();
        if let Some(timestamp) = kept {
            return Ok(timestamp);
        }
        let timestamp = self.chain.timestamp(chain_id, number).await?;
        self.timestamps.lock().insert(key, timestamp);
        Ok(timestamp)
    }
}

/// Add backend telemetry at the verifier boundary, including identity validation.
pub(crate) struct ObservedVerifier<'a>(
    pub &'a xmtp_id::scw_verifier::CachedSmartContractSignatureVerifier,
);

#[xmtp_common::async_trait]
impl xmtp_id::scw_verifier::SmartContractSignatureVerifier for ObservedVerifier<'_> {
    async fn is_valid_signature(
        &self,
        account_id: xmtp_id::associations::AccountId,
        hash: [u8; 32],
        signature: alloy_primitives::Bytes,
        block_number: Option<u64>,
    ) -> Result<xmtp_id::scw_verifier::ValidationResponse, xmtp_id::scw_verifier::VerifierError>
    {
        self.verify(account_id, hash, signature, block_number).await
    }
}
impl ObservedVerifier<'_> {
    /// Preserve the verifier result and its retryability while recording its outcome.
    #[xmtp_common::span(prefix = "scw")]
    async fn verify(
        &self,
        account_id: xmtp_id::associations::AccountId,
        hash: [u8; 32],
        signature: alloy_primitives::Bytes,
        block_number: Option<u64>,
    ) -> Result<xmtp_id::scw_verifier::ValidationResponse, xmtp_id::scw_verifier::VerifierError>
    {
        use crate::telemetry::{self, VerificationResult};
        use xmtp_common::RetryableError;
        use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
        let result = self
            .0
            .is_valid_signature(account_id, hash, signature, block_number)
            .await;
        telemetry::scw_verified(match &result {
            Ok(response) if response.is_valid => VerificationResult::Valid,
            Ok(_) => VerificationResult::Invalid,
            Err(error) if error.is_retryable() => VerificationResult::Error,
            Err(_) => VerificationResult::Invalid,
        });
        result
    }
}

#[cfg(test)]
mod tests {
    use super::PublishChainReads;
    use xmtp_id::scw_verifier::ChainBlocks;
    use xmtp_mls_validation::test_utils::{CountingChain, TestChain};

    /// A failed timestamp read is not kept, so a later update in the same
    /// publish reads the chain again rather than failing on the first error.
    #[xmtp_common::test(unwrap_try = true)]
    async fn publish_chain_reads_keep_only_successful_reads() {
        let chain = TestChain::at(10);
        let counting = CountingChain::new(chain.clone());
        let reads = PublishChainReads::new(&counting);
        chain.set(None);
        assert!(reads.timestamp("eip155:1", 5).await.is_err());
        chain.set(Some(10));
        assert_eq!(reads.timestamp("eip155:1", 5).await?, 5);
        assert_eq!(counting.reads(), (0, 2));
    }
}
