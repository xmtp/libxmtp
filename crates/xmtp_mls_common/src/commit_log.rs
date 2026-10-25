use ed25519_dalek::{DigestSigner, Signature, VerifyingKey};
use prost::Message;
use sha2::{Digest, Sha512};
use xmtp_cryptography::XmtpInstallationCredential;
use xmtp_proto::xmtp::{
    backend::v1::SealedCommitLogEntry, mls::message_contents::PlaintextCommitLogEntry,
};

use crate::group_keys::{
    GroupKeys, OpenError, SealError, commit_log_topic, open_commit_log_entry, seal_commit_log_entry,
};

/// Domain separation for installation signatures on commit-log entries.
pub const COMMIT_LOG_AUTHOR_CONTEXT: &[u8] = b"XMTP commit-log author v1";

/// Bind an entry to the installation publishing it and to its commit-log topic.
// implements: FORK-076
pub fn sign_entry_author(
    keys: &GroupKeys,
    author: &XmtpInstallationCredential,
    entry: &mut PlaintextCommitLogEntry,
) -> Result<(), SealError> {
    entry.author_installation_key = author.public_slice().to_vec();
    entry.author_signature.clear();
    let signature = author
        .with_context(COMMIT_LOG_AUTHOR_CONTEXT)
        .and_then(|context| context.try_sign_digest(author_digest(keys, entry)))
        .map_err(|_| SealError::SelfCheck)?;
    entry.author_signature = signature.to_bytes().to_vec();
    Ok(())
}

fn author_digest(keys: &GroupKeys, entry: &PlaintextCommitLogEntry) -> Sha512 {
    let mut unsigned = entry.clone();
    unsigned.author_signature.clear();
    let mut digest = Sha512::new();
    digest.update(commit_log_topic(&keys.commit_log_id()));
    digest.update(unsigned.encode_to_vec());
    digest
}

/// Seal a conversation's entry for its commit-log topic, after the
/// self-check. The entry's protobuf encoding is the plaintext, so the same
/// entry sealed twice gives the same bytes.
// implements: FORK-074
pub fn seal_entry(
    keys: &GroupKeys,
    entry: &PlaintextCommitLogEntry,
) -> Result<SealedCommitLogEntry, SealError> {
    seal_commit_log_entry(keys, &entry.encode_to_vec())
}

/// Why a reader discards an envelope from a commit-log topic. No variant
/// carries entry or key bytes.
#[derive(Debug, thiserror::Error)]
pub enum DiscardReason {
    /// The entry fails a length check, its signature, or its decryption.
    #[error(transparent)]
    Open(#[from] OpenError),
    /// The decrypted bytes are not a `PlaintextCommitLogEntry`.
    #[error("the decrypted commit-log entry does not decode")]
    Decode,
    /// The entry names another group.
    #[error("the commit-log entry names another group")]
    OtherGroup,
    /// The entry's `commit_sequence_id` is 0.
    #[error("the commit-log entry has commit sequence id 0")]
    NoCommitSequenceId,
    /// A commit must precede the envelope reporting its outcome.
    #[error("the commit does not precede its commit-log envelope")]
    CommitNotBeforeEnvelope,
    /// The author key or signature is absent, malformed, or invalid.
    #[error("the commit-log author signature is invalid")]
    Author,
}

/// Open an entry read from the commit-log topic of the conversation whose
/// keys are `keys` and whose group id is `group_id`. Every length is checked
/// first, then the signature under the conversation's commit-log id, then
/// the decryption. The decrypted entry must name `group_id` and a commit
/// sequence id greater than 0 and below its envelope sequence id, with a
/// valid installation signature. A failure discards this envelope only.
// implements: FORK-077
pub fn open_entry(
    keys: &GroupKeys,
    group_id: &[u8],
    envelope_sequence_id: u64,
    sealed: &SealedCommitLogEntry,
) -> Result<PlaintextCommitLogEntry, DiscardReason> {
    let plaintext = open_commit_log_entry(keys, sealed)?;
    let entry =
        PlaintextCommitLogEntry::decode(plaintext.as_slice()).map_err(|_| DiscardReason::Decode)?;
    if entry.group_id != group_id {
        return Err(DiscardReason::OtherGroup);
    }
    if entry.commit_sequence_id == 0 {
        return Err(DiscardReason::NoCommitSequenceId);
    }
    if entry.commit_sequence_id >= envelope_sequence_id {
        return Err(DiscardReason::CommitNotBeforeEnvelope);
    }
    let author_bytes = entry
        .author_installation_key
        .as_slice()
        .try_into()
        .map_err(|_| DiscardReason::Author)?;
    let author = VerifyingKey::from_bytes(author_bytes).map_err(|_| DiscardReason::Author)?;
    let signature =
        Signature::from_slice(&entry.author_signature).map_err(|_| DiscardReason::Author)?;
    author
        .verify_prehashed_strict(
            author_digest(keys, &entry),
            Some(COMMIT_LOG_AUTHOR_CONTEXT),
            &signature,
        )
        .map_err(|_| DiscardReason::Author)?;
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::group_keys::GroupRoot;

    fn keys() -> GroupKeys {
        GroupKeys::derive(&GroupRoot::from_bytes(&[7; 32]).unwrap())
    }

    fn entry(keys: &GroupKeys, commit_sequence_id: u64) -> PlaintextCommitLogEntry {
        PlaintextCommitLogEntry {
            group_id: keys.group_id().to_vec(),
            commit_sequence_id,
            last_epoch_authenticator: vec![1; 32],
            commit_result: 1,
            applied_epoch_number: 2,
            applied_epoch_authenticator: vec![2; 32],
            ..Default::default()
        }
    }

    /// The same entry sealed twice is the same bytes, and it opens to
    /// itself under its own group.
    // verifies: FORK-074, FORK-076, FORK-077
    #[xmtp_common::test(unwrap_try = true)]
    fn a_sealed_entry_opens_to_itself() {
        let keys = keys();
        let mut entry = entry(&keys, 5);
        sign_entry_author(&keys, &XmtpInstallationCredential::new(), &mut entry)?;
        let sealed = seal_entry(&keys, &entry)?;
        assert_eq!(sealed, seal_entry(&keys, &entry)?);
        assert_eq!(sealed.commit_log_id, keys.commit_log_id());
        assert_eq!(open_entry(&keys, &keys.group_id(), 10, &sealed)?, entry);
    }

    /// The checks after decryption: an entry that names another group,
    /// or commit sequence id 0, is discarded even though it is validly
    /// sealed. Bytes that are not an entry are discarded too.
    // verifies: FORK-077
    #[xmtp_common::test(unwrap_try = true)]
    fn a_validly_sealed_entry_must_name_its_group_and_a_commit() {
        let keys = keys();
        let mut other_group = entry(&keys, 5);
        other_group.group_id = vec![9; 32];
        let sealed = seal_entry(&keys, &other_group)?;
        assert!(matches!(
            open_entry(&keys, &keys.group_id(), 10, &sealed),
            Err(DiscardReason::OtherGroup)
        ));

        let sealed = seal_entry(&keys, &entry(&keys, 0))?;
        assert!(matches!(
            open_entry(&keys, &keys.group_id(), 10, &sealed),
            Err(DiscardReason::NoCommitSequenceId)
        ));

        // A field-1 length prefix that runs past the end does not decode.
        let sealed = seal_commit_log_entry(&keys, &[0x0a, 0x20, 0x01])?;
        assert!(matches!(
            open_entry(&keys, &keys.group_id(), 10, &sealed),
            Err(DiscardReason::Decode)
        ));
    }

    /// An entry of another conversation's commit log, or with a malformed
    /// field, is discarded before decryption, and the reason shows no bytes.
    // verifies: FORK-077
    #[xmtp_common::test(unwrap_try = true)]
    fn an_entry_that_does_not_open_is_discarded() {
        let keys = keys();
        let other = GroupKeys::derive(&GroupRoot::from_bytes(&[8; 32]).unwrap());
        let sealed = seal_entry(&other, &entry(&keys, 5))?;
        let reason = open_entry(&keys, &keys.group_id(), 10, &sealed).unwrap_err();
        assert!(matches!(
            reason,
            DiscardReason::Open(OpenError::OtherCommitLog)
        ));

        let mut short_nonce = seal_entry(&keys, &entry(&keys, 5))?;
        short_nonce.nonce.pop();
        let reason = open_entry(&keys, &keys.group_id(), 10, &short_nonce).unwrap_err();
        assert!(matches!(
            reason,
            DiscardReason::Open(OpenError::Malformed(_))
        ));
        let shown = format!("{reason} {reason:?}");
        assert!(!shown.contains(&hex::encode(&short_nonce.nonce)), "{shown}");
    }

    // verifies: FORK-076, FORK-077
    #[xmtp_common::test(unwrap_try = true)]
    fn an_entry_carries_a_verifiable_author() {
        let keys = keys();
        let author = XmtpInstallationCredential::new();
        let mut entry = entry(&keys, 5);
        sign_entry_author(&keys, &author, &mut entry)?;
        assert_eq!(entry.author_installation_key, author.public_slice());
        let mut unsigned = entry.clone();
        unsigned.author_signature.clear();
        let mut digest = Sha512::new();
        digest.update(commit_log_topic(&keys.commit_log_id()));
        digest.update(unsigned.encode_to_vec());
        author.verifying_key().verify_prehashed_strict(
            digest,
            Some(b"XMTP commit-log author v1"),
            &Signature::from_slice(&entry.author_signature)?,
        )?;
    }

    // verifies: FORK-074, FORK-076
    #[xmtp_common::test(unwrap_try = true)]
    fn a_signed_entry_seals_to_the_same_bytes() {
        let keys = keys();
        let author = XmtpInstallationCredential::new();
        let mut entry = entry(&keys, 5);
        sign_entry_author(&keys, &author, &mut entry)?;
        let first = seal_entry(&keys, &entry)?;
        sign_entry_author(&keys, &author, &mut entry)?;
        assert_eq!(first, seal_entry(&keys, &entry)?);
    }

    // verifies: FORK-077
    #[xmtp_common::test(unwrap_try = true)]
    fn an_entry_without_a_valid_author_is_discarded() {
        let keys = keys();
        let mut signed = entry(&keys, 5);
        sign_entry_author(&keys, &XmtpInstallationCredential::new(), &mut signed)?;
        for case in 0..5 {
            let mut entry = signed.clone();
            match case {
                0 => entry.author_installation_key.clear(),
                1 => entry.author_signature.clear(),
                2 => {
                    entry.author_installation_key =
                        XmtpInstallationCredential::new().public_slice().to_vec()
                }
                3 => entry.applied_epoch_number += 1,
                _ => entry.author_signature[0] ^= 1,
            }
            let sealed = seal_entry(&keys, &entry)?;
            assert!(
                matches!(
                    open_entry(&keys, &keys.group_id(), 10, &sealed),
                    Err(DiscardReason::Author)
                ),
                "case {case}"
            );
        }
    }

    // verifies: FORK-077
    #[xmtp_common::test(unwrap_try = true)]
    fn an_entry_naming_a_later_commit_is_discarded() {
        let keys = keys();
        for commit in [10, 11] {
            let mut entry = entry(&keys, commit);
            sign_entry_author(&keys, &XmtpInstallationCredential::new(), &mut entry)?;
            let sealed = seal_entry(&keys, &entry)?;
            assert!(matches!(
                open_entry(&keys, &keys.group_id(), 10, &sealed),
                Err(DiscardReason::CommitNotBeforeEnvelope)
            ));
        }
    }
}
