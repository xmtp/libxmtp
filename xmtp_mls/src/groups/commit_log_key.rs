use crate::groups::MlsGroup;
use crate::groups::XmtpSharedContext;
use crate::groups::commit_log::CommitLogError;
use openmls::prelude::{OpenMlsCrypto, SignatureScheme};
use openmls_rust_crypto::RustCrypto;
use openmls_traits::OpenMlsProvider;
use xmtp_cryptography::Secret;
use xmtp_db::MlsProviderExt;
use xmtp_db::group::StoredGroupCommitLogPublicKey;
use xmtp_db::prelude::QueryGroup;
use xmtp_db::{
    XmtpMlsStorageProvider,
    sql_key_store::{COMMIT_LOG_SIGNER_PRIVATE_KEY, SqlKeyStoreError},
};
use xmtp_proto::xmtp::mls::api::v1::QueryCommitLogResponse;
use xmtp_proto::xmtp::mls::message_contents::CommitLogEntry as CommitLogEntryProto;

pub(crate) trait CommitLogKeyCrypto {
    type Error: std::error::Error;
    fn generate_commit_log_key(&self) -> Result<Secret, Self::Error>;
    fn public_key_matches_private_key(public_key: &[u8], private_key: &Secret) -> bool;
    fn verify_commit_log_signature(
        &self,
        entry: &CommitLogEntryProto,
        expected_public_key: &[u8],
    ) -> Result<(), Self::Error>;
}

impl CommitLogKeyCrypto for RustCrypto {
    type Error = openmls_traits::types::CryptoError;
    fn generate_commit_log_key(&self) -> Result<Secret, Self::Error> {
        let (private_key, _) = self.signature_key_gen(SignatureScheme::ED25519)?;
        Ok(Secret::new(private_key))
    }

    fn public_key_matches_private_key(public_key: &[u8], private_key: &Secret) -> bool {
        let Ok(computed_public_key) = xmtp_cryptography::signature::to_public_key(private_key)
        else {
            tracing::warn!("Invalid private key length");
            return false;
        };
        public_key == computed_public_key
    }

    fn verify_commit_log_signature(
        &self,
        entry: &CommitLogEntryProto,
        expected_public_key: &[u8],
    ) -> Result<(), Self::Error> {
        let Some(signature) = &entry.signature else {
            return Err(openmls_traits::types::CryptoError::InvalidSignature);
        };
        if signature.public_key != expected_public_key {
            return Err(openmls_traits::types::CryptoError::InvalidSignature);
        }
        self.verify_signature(
            SignatureScheme::ED25519,
            entry.serialized_commit_log_entry.as_slice(),
            expected_public_key,
            &signature.bytes,
        )?;
        Ok(())
    }
}

pub(crate) trait CommitLogKeyStore {
    type Error: std::error::Error;
    fn read_commit_log_key(&self, group_id: &[u8]) -> Result<Option<Secret>, Self::Error>;
    fn write_commit_log_key(&self, group_id: &[u8], value: &Secret) -> Result<(), Self::Error>;
}

impl<KeyStore: XmtpMlsStorageProvider> CommitLogKeyStore for KeyStore {
    type Error = SqlKeyStoreError;

    fn read_commit_log_key(&self, group_id: &[u8]) -> Result<Option<Secret>, Self::Error> {
        let key = bincode::serialize(group_id)?;
        let value = self
            .read::<Vec<u8>>(COMMIT_LOG_SIGNER_PRIVATE_KEY, &key)?
            .map(Secret::new);
        Ok(value)
    }

    fn write_commit_log_key(&self, group_id: &[u8], value: &Secret) -> Result<(), Self::Error> {
        let key = bincode::serialize(group_id)?;
        let value = Secret::new(bincode::serialize(value.as_slice())?);
        self.write(COMMIT_LOG_SIGNER_PRIVATE_KEY, &key, value.as_slice())
    }
}

pub(crate) fn derive_consensus_public_key(
    context: &impl XmtpSharedContext,
    commit_log_response: &QueryCommitLogResponse,
) -> Result<Option<Vec<u8>>, CommitLogError> {
    let provider = context.mls_provider();
    // Find the first entry with a valid signature and extract its public key
    for entry in &commit_log_response.commit_log_entries {
        if let Some(signature) = &entry.signature
            && provider
                .crypto()
                .verify_commit_log_signature(entry, &signature.public_key)
                .is_ok()
        {
            context.db().set_group_commit_log_public_key(
                &commit_log_response.group_id,
                &signature.public_key,
            )?;
            return Ok(Some(signature.public_key.clone()));
        }
    }

    tracing::warn!(
        "No valid signature found in commit log response for group {:?}",
        hex::encode(&commit_log_response.group_id)
    );
    Ok(None)
}

// TODO(rich): Handle race conditions where commit log key can be overwritten
pub(crate) fn get_or_create_signing_key(
    context: &impl XmtpSharedContext,
    conversation: &StoredGroupCommitLogPublicKey,
) -> Result<Option<Secret>, CommitLogError> {
    let provider = context.mls_provider();
    let key_store = provider.key_store();
    // The consensus_public_key is derived from the first entry in the commit log, if one has been previously received.
    // If there is one, we try to find the private key in the key store, and then the mutable metadata, returning None if not found.
    // If there is none, we use any existing private key from the same locations, creating a new key if not found.
    let consensus_public_key = conversation.commit_log_public_key.as_ref();

    if let Some(private_key) = key_store.read_commit_log_key(&conversation.id)?
        && consensus_public_key.is_none_or(|consensus_public_key| {
            RustCrypto::public_key_matches_private_key(consensus_public_key, &private_key)
        })
    {
        return Ok(Some(private_key));
    }

    let (group, _) = MlsGroup::new_cached(context, &conversation.id)?;
    if let Some(private_key) = group.mutable_metadata()?.commit_log_signer
        && consensus_public_key.is_none_or(|consensus_public_key| {
            RustCrypto::public_key_matches_private_key(consensus_public_key, &private_key)
        })
    {
        key_store.write_commit_log_key(&conversation.id, &private_key)?;
        return Ok(Some(private_key));
    }

    if consensus_public_key.is_none() {
        // We have not yet seen an agreed upon public key for this conversation, so we generate a new key.
        // We store the key locally, but do not share it via mutable metadata until we verify that we
        // published it as the first commit log entry.
        let private_key = provider.crypto().generate_commit_log_key()?;
        key_store.write_commit_log_key(&conversation.id, &private_key)?;
        return Ok(Some(private_key));
    }

    tracing::warn!(
        "Commit log consensus key {:?} is not available yet for conversation {:?}",
        consensus_public_key.map(hex::encode),
        hex::encode(&conversation.id)
    );
    Ok(None)
}

#[cfg(test)]
mod tests {
    use xmtp_db::MlsProviderExt;

    use super::*;
    use crate::tester;

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_read_write_commit_log_key() {
        tester!(alix);
        let provider = alix.context.mls_provider();
        let key_store = provider.key_store();

        key_store.write_commit_log_key(&[1u8; 32], &Secret::new(vec![10u8; 32]))?;

        // Query on a value that hasn't been written
        let result = key_store.read_commit_log_key(&[2u8; 32]);
        assert!(result.is_ok(), "{}", result.err().unwrap());
        assert!(result.unwrap().is_none());

        let result = key_store.read_commit_log_key(&[1u8; 32]);
        assert!(result.is_ok(), "{}", result.err().unwrap());
        assert_eq!(result.unwrap().unwrap().as_slice(), &[10u8; 32]);
    }
}
