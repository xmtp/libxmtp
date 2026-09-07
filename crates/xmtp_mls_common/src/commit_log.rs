use openmls::prelude::{OpenMlsCrypto, SignatureScheme};
use prost::Message;
use xmtp_cryptography::Secret;
use xmtp_proto::xmtp::{
    identity::associations::RecoverableEd25519Signature,
    mls::message_contents::PlaintextCommitLogEntry,
};

/// Decode a commit-log entry without checking its signature or hash chain.
pub fn decode_commit_log(data: &[u8]) -> Result<PlaintextCommitLogEntry, prost::DecodeError> {
    PlaintextCommitLogEntry::decode(data)
}

pub struct SignedCommitLogEntry {
    pub serialized_commit_log_entry: Vec<u8>,
    pub signature: RecoverableEd25519Signature,
}

#[derive(Debug, thiserror::Error)]
pub enum CommitLogSigningError {
    /// Signing failed. Not retryable.
    #[error(transparent)]
    Crypto(#[from] openmls::prelude::CryptoError),
    /// The signing key has an invalid length. Not retryable.
    #[error(transparent)]
    KeyLength(#[from] std::array::TryFromSliceError),
}

/// Construct signed protocol bytes. The caller owns key storage and publication.
pub fn sign_commit_log(
    entry: &PlaintextCommitLogEntry,
    private_key: &Secret,
    crypto: &impl OpenMlsCrypto,
) -> Result<SignedCommitLogEntry, CommitLogSigningError> {
    let serialized_commit_log_entry = entry.encode_to_vec();
    let bytes = crypto.sign(
        SignatureScheme::ED25519,
        &serialized_commit_log_entry,
        private_key.as_slice(),
    )?;
    let public_key = xmtp_cryptography::signature::to_public_key(private_key)?.to_vec();
    Ok(SignedCommitLogEntry {
        serialized_commit_log_entry,
        signature: RecoverableEd25519Signature { bytes, public_key },
    })
}
