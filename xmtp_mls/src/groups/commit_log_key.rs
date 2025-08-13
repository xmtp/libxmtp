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
    use xmtp_proto::xmtp::mls::message_contents::CommitLogEntry as CommitLogEntryProto;

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

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_verify_commit_log_signature() {
        tester!(alix);
        let provider = alix.context.mls_provider();
        let crypto = provider.crypto();

        let private_key = crypto.generate_commit_log_key().unwrap();
        let public_key = xmtp_cryptography::signature::to_public_key(&private_key)
            .unwrap()
            .to_vec();

        let message = b"test message";
        let signature_bytes = crypto
            .sign(
                openmls::prelude::SignatureScheme::ED25519,
                message,
                private_key.as_slice(),
            )
            .unwrap();

        let commit_entry = CommitLogEntryProto {
            sequence_id: 1,
            serialized_commit_log_entry: message.to_vec(),
            signature: Some(
                xmtp_proto::xmtp::identity::associations::RecoverableEd25519Signature {
                    public_key: public_key.clone(),
                    bytes: signature_bytes,
                },
            ),
        };

        // Valid signature should verify
        assert!(
            crypto
                .verify_commit_log_signature(&commit_entry, &public_key)
                .is_ok()
        );

        // Wrong public key should fail
        let wrong_public_key = vec![0u8; 32];
        assert!(
            crypto
                .verify_commit_log_signature(&commit_entry, &wrong_public_key)
                .is_err()
        );

        // Entry without signature should fail
        let unsigned_entry = CommitLogEntryProto {
            sequence_id: 1,
            serialized_commit_log_entry: message.to_vec(),
            signature: None,
        };
        assert!(
            crypto
                .verify_commit_log_signature(&unsigned_entry, &public_key)
                .is_err()
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_derive_consensus_public_key_with_valid_signature() {
        tester!(alix);
        let provider = alix.context.mls_provider();
        let crypto = provider.crypto();

        // Use an actual group ID to avoid database conflicts
        let group = alix.create_group(None, None).unwrap();

        // Create first key pair (this should be chosen as consensus key)
        let first_private_key = crypto.generate_commit_log_key().unwrap();
        let first_public_key = xmtp_cryptography::signature::to_public_key(&first_private_key)
            .unwrap()
            .to_vec();

        // Create second key pair (this should be ignored)
        let second_private_key = crypto.generate_commit_log_key().unwrap();
        let second_public_key = xmtp_cryptography::signature::to_public_key(&second_private_key)
            .unwrap()
            .to_vec();

        let first_message = b"first commit";
        let first_signature = crypto
            .sign(
                openmls::prelude::SignatureScheme::ED25519,
                first_message,
                first_private_key.as_slice(),
            )
            .unwrap();

        let second_message = b"second commit";
        let second_signature = crypto
            .sign(
                openmls::prelude::SignatureScheme::ED25519,
                second_message,
                second_private_key.as_slice(),
            )
            .unwrap();

        let first_entry = CommitLogEntryProto {
            sequence_id: 1,
            serialized_commit_log_entry: first_message.to_vec(),
            signature: Some(
                xmtp_proto::xmtp::identity::associations::RecoverableEd25519Signature {
                    public_key: first_public_key.clone(),
                    bytes: first_signature,
                },
            ),
        };

        let second_entry = CommitLogEntryProto {
            sequence_id: 2,
            serialized_commit_log_entry: second_message.to_vec(),
            signature: Some(
                xmtp_proto::xmtp::identity::associations::RecoverableEd25519Signature {
                    public_key: second_public_key.clone(),
                    bytes: second_signature,
                },
            ),
        };

        let response = xmtp_proto::xmtp::mls::api::v1::QueryCommitLogResponse {
            group_id: group.group_id.clone(),
            commit_log_entries: vec![first_entry, second_entry],
            paging_info: None,
        };

        let result = derive_consensus_public_key(&alix.context, &response).unwrap();
        assert!(result.is_some());
        let consensus_key = result.unwrap();
        // Should return the FIRST valid public key, not the second
        assert_eq!(consensus_key, first_public_key);
        assert_ne!(consensus_key, second_public_key);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_derive_consensus_public_key_with_no_valid_signature() {
        tester!(alix);
        let provider = alix.context.mls_provider();
        let crypto = provider.crypto();

        // Use an actual group ID to avoid database conflicts
        let group = alix.create_group(None, None).unwrap();

        // Create a valid second entry
        let valid_private_key = crypto.generate_commit_log_key().unwrap();
        let valid_public_key = xmtp_cryptography::signature::to_public_key(&valid_private_key)
            .unwrap()
            .to_vec();

        let valid_message = b"valid commit";
        let valid_signature = crypto
            .sign(
                openmls::prelude::SignatureScheme::ED25519,
                valid_message,
                valid_private_key.as_slice(),
            )
            .unwrap();

        // First entry has no signature (should be skipped)
        let unsigned_entry = CommitLogEntryProto {
            sequence_id: 1,
            serialized_commit_log_entry: b"unsigned commit".to_vec(),
            signature: None,
        };

        // Second entry has valid signature (should be used)
        let valid_entry = CommitLogEntryProto {
            sequence_id: 2,
            serialized_commit_log_entry: valid_message.to_vec(),
            signature: Some(
                xmtp_proto::xmtp::identity::associations::RecoverableEd25519Signature {
                    public_key: valid_public_key.clone(),
                    bytes: valid_signature,
                },
            ),
        };

        let response = xmtp_proto::xmtp::mls::api::v1::QueryCommitLogResponse {
            group_id: group.group_id.clone(),
            commit_log_entries: vec![unsigned_entry, valid_entry],
            paging_info: None,
        };

        let result = derive_consensus_public_key(&alix.context, &response).unwrap();
        assert!(result.is_some());
        // Should derive from the second entry (first valid one)
        assert_eq!(result.unwrap(), valid_public_key);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_derive_consensus_public_key_with_invalid_signature() {
        tester!(alix);
        let provider = alix.context.mls_provider();
        let crypto = provider.crypto();

        // Use an actual group ID to avoid database conflicts
        let group = alix.create_group(None, None).unwrap();

        // Create keys for invalid first entry
        let invalid_private_key = crypto.generate_commit_log_key().unwrap();
        let invalid_public_key = xmtp_cryptography::signature::to_public_key(&invalid_private_key)
            .unwrap()
            .to_vec();

        // Create valid second entry
        let valid_private_key = crypto.generate_commit_log_key().unwrap();
        let valid_public_key = xmtp_cryptography::signature::to_public_key(&valid_private_key)
            .unwrap()
            .to_vec();

        let valid_message = b"valid commit";
        let valid_signature = crypto
            .sign(
                openmls::prelude::SignatureScheme::ED25519,
                valid_message,
                valid_private_key.as_slice(),
            )
            .unwrap();

        // First entry with invalid signature (should be skipped)
        let invalid_entry = CommitLogEntryProto {
            sequence_id: 1,
            serialized_commit_log_entry: b"invalid commit".to_vec(),
            signature: Some(
                xmtp_proto::xmtp::identity::associations::RecoverableEd25519Signature {
                    public_key: invalid_public_key.clone(),
                    bytes: vec![0u8; 64], // Invalid signature bytes
                },
            ),
        };

        // Second entry with valid signature (should be used)
        let valid_entry = CommitLogEntryProto {
            sequence_id: 2,
            serialized_commit_log_entry: valid_message.to_vec(),
            signature: Some(
                xmtp_proto::xmtp::identity::associations::RecoverableEd25519Signature {
                    public_key: valid_public_key.clone(),
                    bytes: valid_signature,
                },
            ),
        };

        let response = xmtp_proto::xmtp::mls::api::v1::QueryCommitLogResponse {
            group_id: group.group_id.clone(),
            commit_log_entries: vec![invalid_entry, valid_entry],
            paging_info: None,
        };

        let result = derive_consensus_public_key(&alix.context, &response).unwrap();
        assert!(result.is_some());
        let consensus_key = result.unwrap();
        // Should derive from the second entry (first valid one), not the invalid first one
        assert_eq!(consensus_key, valid_public_key);
        assert_ne!(consensus_key, invalid_public_key);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_get_or_create_signing_key_uses_mutable_metadata() {
        tester!(alix);

        // Create a group - this will have a commit_log_signer in mutable metadata by default
        let group = alix.create_group(None, None).unwrap();
        let metadata = group.mutable_metadata().unwrap();
        let mutable_metadata_key = metadata.commit_log_signer.unwrap();

        let conversation = StoredGroupCommitLogPublicKey {
            id: group.group_id.clone(),
            commit_log_public_key: None, // No consensus key
        };

        let key = get_or_create_signing_key(&alix.context, &conversation).unwrap();
        assert!(key.is_some());
        // Should return the key from mutable metadata
        assert_eq!(key.unwrap().as_slice(), mutable_metadata_key.as_slice());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_get_or_create_signing_key_ignores_non_matching_consensus() {
        tester!(alix);
        let provider = alix.context.mls_provider();
        let crypto = provider.crypto();
        let key_store = provider.key_store();

        let group = alix.create_group(None, None).unwrap();

        // Store a key that doesn't match the consensus
        let stored_key = crypto.generate_commit_log_key().unwrap();
        key_store
            .write_commit_log_key(&group.group_id, &stored_key)
            .unwrap();

        // Set a different consensus key
        let consensus_key = crypto.generate_commit_log_key().unwrap();
        let consensus_public_key = xmtp_cryptography::signature::to_public_key(&consensus_key)
            .unwrap()
            .to_vec();

        let conversation = StoredGroupCommitLogPublicKey {
            id: group.group_id.clone(),
            commit_log_public_key: Some(consensus_public_key),
        };

        let key = get_or_create_signing_key(&alix.context, &conversation).unwrap();
        // Should return None because stored key doesn't match consensus
        assert!(key.is_none());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_get_or_create_signing_key_uses_matching_stored_key() {
        tester!(alix);
        let provider = alix.context.mls_provider();
        let crypto = provider.crypto();
        let key_store = provider.key_store();

        let group = alix.create_group(None, None).unwrap();

        // Store a key
        let stored_key = crypto.generate_commit_log_key().unwrap();
        let stored_public_key = xmtp_cryptography::signature::to_public_key(&stored_key)
            .unwrap()
            .to_vec();
        key_store
            .write_commit_log_key(&group.group_id, &stored_key)
            .unwrap();

        // Set consensus key that matches the stored key
        let conversation = StoredGroupCommitLogPublicKey {
            id: group.group_id.clone(),
            commit_log_public_key: Some(stored_public_key),
        };

        let key = get_or_create_signing_key(&alix.context, &conversation).unwrap();
        assert!(key.is_some());
        // Should return the stored key that matches consensus
        assert_eq!(key.unwrap().as_slice(), stored_key.as_slice());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_get_or_create_signing_key_uses_matching_mutable_metadata() {
        tester!(alix);

        let group = alix.create_group(None, None).unwrap();
        let metadata = group.mutable_metadata().unwrap();
        let metadata_key = metadata.commit_log_signer.unwrap();
        let metadata_public_key = xmtp_cryptography::signature::to_public_key(&metadata_key)
            .unwrap()
            .to_vec();

        // Set consensus key that matches the mutable metadata key
        let conversation = StoredGroupCommitLogPublicKey {
            id: group.group_id.clone(),
            commit_log_public_key: Some(metadata_public_key),
        };

        let key = get_or_create_signing_key(&alix.context, &conversation).unwrap();
        assert!(key.is_some());
        // Should return the key from mutable metadata that matches consensus
        assert_eq!(key.unwrap().as_slice(), metadata_key.as_slice());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn test_get_or_create_signing_key_returns_none_with_consensus_no_matching_key() {
        tester!(alix);
        let provider = alix.context.mls_provider();
        let crypto = provider.crypto();
        let key_store = provider.key_store();

        let group = alix.create_group(None, None).unwrap();
        let group_id = group.group_id.clone();

        // Clear the key store to ensure no stored key exists
        key_store
            .delete::<1>(
                xmtp_db::sql_key_store::COMMIT_LOG_SIGNER_PRIVATE_KEY,
                &bincode::serialize(&group_id).unwrap(),
            )
            .ok();

        // Set a consensus key that we don't have the private key for
        let consensus_key = crypto.generate_commit_log_key().unwrap();
        let consensus_public_key = xmtp_cryptography::signature::to_public_key(&consensus_key)
            .unwrap()
            .to_vec();

        let conversation = StoredGroupCommitLogPublicKey {
            id: group_id,
            commit_log_public_key: Some(consensus_public_key),
        };

        let key = get_or_create_signing_key(&alix.context, &conversation).unwrap();
        // Should return None because:
        // 1. No key exists in the key store
        // 2. Mutable metadata key doesn't match consensus
        // 3. We have a consensus key so we can't create a new one
        assert!(key.is_none());
    }
}
