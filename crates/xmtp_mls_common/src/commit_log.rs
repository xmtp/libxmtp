use prost::Message;
use xmtp_proto::xmtp::{
    backend::v1::SealedCommitLogEntry, mls::message_contents::PlaintextCommitLogEntry,
};

use crate::group_keys::{
    GroupKeys, OpenError, SealError, open_commit_log_entry, seal_commit_log_entry,
};

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
}

/// Open an entry read from the commit-log topic of the conversation whose
/// keys are `keys` and whose group id is `group_id`. Every length is checked
/// first, then the signature under the conversation's commit-log id, then
/// the decryption. The decrypted entry must name `group_id` and a commit
/// sequence id greater than 0. A failure discards this envelope only.
// implements: FORK-075
pub fn open_entry(
    keys: &GroupKeys,
    group_id: &[u8],
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
        }
    }

    /// The same entry sealed twice is the same bytes, and it opens to
    /// itself under its own group.
    // verifies: FORK-074, FORK-075
    #[xmtp_common::test(unwrap_try = true)]
    fn a_sealed_entry_opens_to_itself() {
        let keys = keys();
        let entry = entry(&keys, 5);
        let sealed = seal_entry(&keys, &entry)?;
        assert_eq!(sealed, seal_entry(&keys, &entry)?);
        assert_eq!(sealed.commit_log_id, keys.commit_log_id());
        assert_eq!(open_entry(&keys, &keys.group_id(), &sealed)?, entry);
    }

    /// The checks after decryption: an entry that names another group,
    /// or commit sequence id 0, is discarded even though it is validly
    /// sealed. Bytes that are not an entry are discarded too.
    // verifies: FORK-075
    #[xmtp_common::test(unwrap_try = true)]
    fn a_validly_sealed_entry_must_name_its_group_and_a_commit() {
        let keys = keys();
        let mut other_group = entry(&keys, 5);
        other_group.group_id = vec![9; 32];
        let sealed = seal_entry(&keys, &other_group)?;
        assert!(matches!(
            open_entry(&keys, &keys.group_id(), &sealed),
            Err(DiscardReason::OtherGroup)
        ));

        let sealed = seal_entry(&keys, &entry(&keys, 0))?;
        assert!(matches!(
            open_entry(&keys, &keys.group_id(), &sealed),
            Err(DiscardReason::NoCommitSequenceId)
        ));

        // A field-1 length prefix that runs past the end does not decode.
        let sealed = seal_commit_log_entry(&keys, &[0x0a, 0x20, 0x01])?;
        assert!(matches!(
            open_entry(&keys, &keys.group_id(), &sealed),
            Err(DiscardReason::Decode)
        ));
    }

    /// An entry of another conversation's commit log, or with a malformed
    /// field, is discarded before decryption, and the reason shows no bytes.
    // verifies: FORK-075
    #[xmtp_common::test(unwrap_try = true)]
    fn an_entry_that_does_not_open_is_discarded() {
        let keys = keys();
        let other = GroupKeys::derive(&GroupRoot::from_bytes(&[8; 32]).unwrap());
        let sealed = seal_entry(&other, &entry(&keys, 5))?;
        let reason = open_entry(&keys, &keys.group_id(), &sealed).unwrap_err();
        assert!(matches!(
            reason,
            DiscardReason::Open(OpenError::OtherCommitLog)
        ));

        let mut short_nonce = seal_entry(&keys, &entry(&keys, 5))?;
        short_nonce.nonce.pop();
        let reason = open_entry(&keys, &keys.group_id(), &short_nonce).unwrap_err();
        assert!(matches!(
            reason,
            DiscardReason::Open(OpenError::Malformed(_))
        ));
        let shown = format!("{reason} {reason:?}");
        assert!(!shown.contains(&hex::encode(&short_nonce.nonce)), "{shown}");
    }
}
