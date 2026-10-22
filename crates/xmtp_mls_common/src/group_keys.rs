//! Keys derived from a group root, the structures a publish signature covers,
//! and sealed commit-log entries.
//!
//! A group root is 32 secret bytes drawn once per group. HKDF-SHA256 derives
//! one key per use from it ([`GroupKeys`]). The group signing key's public key
//! is the group id, and the commit-log signing key's public key is the
//! commit-log id. The backend and every client verify a publish signature the
//! same way ([`verify_publish_signature`]).
//!
//! Ed25519 signing is deterministic, so every retry and every publisher signs
//! the same bytes. One faulty signature next to a correct copy of the same
//! signed bytes reveals the signing key. So nothing is released until a
//! self-check passes: sign twice and require equal bytes, then verify as the
//! backend does, over signed bytes rebuilt from the envelope's own fields. A
//! sealed entry is also opened again, and the nonce derived from the decrypted
//! bytes must equal the envelope's nonce.

use std::{borrow::Cow, fmt};

use curve25519_dalek::edwards::CompressedEdwardsY;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use openmls_traits::{
    crypto::OpenMlsCrypto,
    types::{AeadType, CryptoError},
};
use sha2::Sha256;
use tls_codec::{Serialize as _, Size as _, VLByteSlice};
use xmtp_common::RetryableError;
use xmtp_cryptography::secret::Zeroizing;
use xmtp_proto::{
    types::TopicKind,
    xmtp::backend::v1::{GroupMessage, SealedCommitLogEntry},
};

use crate::mls_ext::payload_encryption::LIBCRUX_CRYPTO_PROVIDER;

/// Length of a group root.
pub const GROUP_ROOT_BYTES: usize = 32;
/// Length of an Ed25519 public key: a group id or a commit-log id.
pub const PUBLIC_KEY_BYTES: usize = ed25519_dalek::PUBLIC_KEY_LENGTH;
/// Length of a publish signature, `R || S`.
pub const PUBLISH_SIGNATURE_BYTES: usize = ed25519_dalek::SIGNATURE_LENGTH;
/// Length of a sealed commit-log entry's nonce.
pub const COMMIT_LOG_NONCE_BYTES: usize = 12;
/// Length of the ChaCha20-Poly1305 tag, so the shortest sealed ciphertext.
pub const COMMIT_LOG_TAG_BYTES: usize = 16;
/// Length of a keyed topic: the kind byte and a public key.
pub const KEYED_TOPIC_BYTES: usize = 1 + PUBLIC_KEY_BYTES;

/// Length of each key derived from a root.
const DERIVED_KEY_BYTES: usize = 32;

/// The HKDF salt, and the HKDF `info` of each key in the group key table.
const GROUP_ROOT_SALT: &[u8] = b"xmtp group root";
const GROUP_SIGNING_KEY_INFO: &[u8] = b"xmtp group signing key";
const COMMIT_LOG_SIGNING_KEY_INFO: &[u8] = b"xmtp commit log signing key";
const COMMIT_LOG_ENCRYPTION_KEY_INFO: &[u8] = b"xmtp commit log encryption key";
const COMMIT_LOG_NONCE_KEY_INFO: &[u8] = b"xmtp commit log nonce key";

/// The label that opens each signed structure.
const GROUP_MESSAGE_LABEL: &[u8] = b"xmtp group message";
const COMMIT_LOG_ENTRY_LABEL: &[u8] = b"xmtp commit log entry";

const COMMIT_LOG_AEAD: AeadType = AeadType::ChaCha20Poly1305;

/// The 32-byte secret that a group's keys derive from.
///
/// Wiped on drop. Neither `Copy` nor `Clone`, so no unwiped copy outlives it,
/// and its `Debug` shows nothing.
pub struct GroupRoot(Zeroizing<[u8; GROUP_ROOT_BYTES]>);

impl GroupRoot {
    /// Draw a root for one new group from the CSPRNG.
    pub fn generate() -> Self {
        let secret = xmtp_cryptography::rand::rand_secret::<GROUP_ROOT_BYTES>();
        let mut root = Zeroizing::new([0; GROUP_ROOT_BYTES]);
        root.copy_from_slice(secret.as_slice());
        Self(root)
    }

    /// Read a stored root, such as a group's `GROUP_ROOT_KEY` component.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, GroupRootLengthError> {
        if bytes.len() != GROUP_ROOT_BYTES {
            return Err(GroupRootLengthError { len: bytes.len() });
        }
        let mut root = Zeroizing::new([0; GROUP_ROOT_BYTES]);
        root.copy_from_slice(bytes);
        Ok(Self(root))
    }

    /// Borrow the root, to store it in the group's AppData dictionary.
    pub fn as_bytes(&self) -> &[u8; GROUP_ROOT_BYTES] {
        &self.0
    }
}

impl fmt::Debug for GroupRoot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GroupRoot").finish_non_exhaustive()
    }
}

/// A stored group root is not 32 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a group root is 32 bytes, not {len}")]
pub struct GroupRootLengthError {
    pub len: usize,
}

/// The keys of the group key table, derived from one group root.
///
/// Every key is wiped on drop, and `Debug` shows none of them. The fields
/// stay private: a caller signs and seals only through this module, so every
/// signature passes the self-check.
pub struct GroupKeys {
    group_signing: SigningKey,
    commit_log_signing: SigningKey,
    commit_log_encryption: Zeroizing<[u8; DERIVED_KEY_BYTES]>,
    commit_log_nonce: Zeroizing<[u8; DERIVED_KEY_BYTES]>,
}

impl GroupKeys {
    /// Derive each key as HKDF-Expand(HKDF-Extract(salt, root), info, 32).
    // implements: GMOD-041
    pub fn derive(root: &GroupRoot) -> Self {
        // A stack local: hkdf does not wipe the state it keys with the PRK.
        let hkdf = Hkdf::<Sha256>::new(Some(GROUP_ROOT_SALT), root.as_bytes());
        let expand = |info: &[u8]| {
            let mut key = Zeroizing::new([0; DERIVED_KEY_BYTES]);
            hkdf.expand(info, &mut key[..])
                .expect("32 bytes is a valid HKDF-SHA256 output length");
            key
        };
        Self {
            group_signing: SigningKey::from_bytes(&expand(GROUP_SIGNING_KEY_INFO)),
            commit_log_signing: SigningKey::from_bytes(&expand(COMMIT_LOG_SIGNING_KEY_INFO)),
            commit_log_encryption: expand(COMMIT_LOG_ENCRYPTION_KEY_INFO),
            commit_log_nonce: expand(COMMIT_LOG_NONCE_KEY_INFO),
        }
    }

    /// The group id: the group signing key's public key.
    pub fn group_id(&self) -> [u8; PUBLIC_KEY_BYTES] {
        self.group_signing.verifying_key().to_bytes()
    }

    /// The commit-log id: the commit-log signing key's public key.
    pub fn commit_log_id(&self) -> [u8; PUBLIC_KEY_BYTES] {
        self.commit_log_signing.verifying_key().to_bytes()
    }
}

impl fmt::Debug for GroupKeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GroupKeys").finish_non_exhaustive()
    }
}

/// The commit-log topic: kind `0x04` followed by the commit-log id. The
/// nonce input, the associated data, the signed structure, and the topic a
/// client reads all use it.
pub fn commit_log_topic(commit_log_id: &[u8; PUBLIC_KEY_BYTES]) -> [u8; KEYED_TOPIC_BYTES] {
    keyed_topic(TopicKind::CommitLogEntriesV1, commit_log_id)
}

fn group_message_topic(group_id: &[u8; PUBLIC_KEY_BYTES]) -> [u8; KEYED_TOPIC_BYTES] {
    keyed_topic(TopicKind::GroupMessagesV1, group_id)
}

fn keyed_topic(kind: TopicKind, id: &[u8; PUBLIC_KEY_BYTES]) -> [u8; KEYED_TOPIC_BYTES] {
    let mut topic = [0; KEYED_TOPIC_BYTES];
    topic[0] = kind as u8;
    topic[1..].copy_from_slice(id);
    topic
}

/// The TLS encoding of `GroupMessageTBS` (API section 3.1), which a group
/// message's `signature` covers. It fails only for a field of 2^30 bytes or
/// more.
pub fn group_message_tbs(
    group_id: &[u8; PUBLIC_KEY_BYTES],
    data: &[u8],
    sender_hmac: &[u8],
    should_push: bool,
) -> Result<Vec<u8>, tls_codec::Error> {
    let topic = group_message_topic(group_id);
    let mut tbs = tls_vectors(&[GROUP_MESSAGE_LABEL, &topic, data, sender_hmac], 1)?;
    u8::from(should_push).tls_serialize(&mut tbs)?;
    Ok(tbs)
}

/// The TLS encoding of `CommitLogEntryTBS` (API section 3.1), which a sealed
/// commit-log entry's `signature` covers. It fails only for a ciphertext of
/// 2^30 bytes or more.
pub fn commit_log_entry_tbs(
    commit_log_id: &[u8; PUBLIC_KEY_BYTES],
    nonce: &[u8; COMMIT_LOG_NONCE_BYTES],
    ciphertext: &[u8],
) -> Result<Vec<u8>, tls_codec::Error> {
    let topic = commit_log_topic(commit_log_id);
    tls_vectors(&[COMMIT_LOG_ENTRY_LABEL, &topic, nonce, ciphertext], 0)
}

/// Each field as an `opaque<V>`, the variable-length vector of RFC 9420
/// section 2.1.2, with room for `trailer` more bytes.
fn tls_vectors(fields: &[&[u8]], trailer: usize) -> Result<Vec<u8>, tls_codec::Error> {
    let len = fields
        .iter()
        .map(|field| VLByteSlice(field).tls_serialized_len())
        .sum::<usize>();
    let mut out = Vec::with_capacity(len + trailer);
    for field in fields {
        VLByteSlice(field).tls_serialize(&mut out)?;
    }
    Ok(out)
}

/// Verify a publish signature over `tbs` under `public_key`.
///
/// The key must be the canonical encoding of a point. `verify_strict` then
/// refuses a small-order key or `R`, a non-canonical `R`, and `S` not less
/// than the group order, and checks the equation without the cofactor. Never
/// replace it with `verify` or batch verification: both accept a small-order
/// key, under which anyone can forge.
// implements: API-300
pub fn verify_publish_signature(
    public_key: &[u8; PUBLIC_KEY_BYTES],
    tbs: &[u8],
    signature: &[u8],
) -> Result<(), PublishSignatureError> {
    let signature: &[u8; PUBLISH_SIGNATURE_BYTES] = signature
        .try_into()
        .map_err(|_| PublishSignatureError::SignatureLength)?;
    // `verify_strict` decodes a key without reducing y and hashes the key
    // bytes as given, so without this check a second encoding of one key
    // would verify and name a second topic. A round trip through
    // `VerifyingKey::from_bytes(..).to_bytes()` is no check: `to_bytes`
    // returns the input.
    let point = CompressedEdwardsY(*public_key)
        .decompress()
        .filter(|point| point.compress().as_bytes() == public_key)
        .ok_or(PublishSignatureError::NonCanonicalKey)?;
    VerifyingKey::from(point)
        .verify_strict(tbs, &Signature::from_bytes(signature))
        .map_err(|_| PublishSignatureError::Invalid)
}

/// Sign a group message under the group signing key and set its `signature`,
/// after the self-check. On an error the message is unchanged.
pub fn sign_group_message(keys: &GroupKeys, message: &mut GroupMessage) -> Result<(), SealError> {
    sign_group_message_with(keys, message, &mut NoFault)
}

fn sign_group_message_with(
    keys: &GroupKeys,
    message: &mut GroupMessage,
    seam: &mut impl FaultSeam,
) -> Result<(), SealError> {
    let group_id = keys.group_id();
    let mut tbs = group_message_tbs(
        &group_id,
        &message.data,
        &message.sender_hmac,
        message.should_push,
    )?;
    let signature = sign_twice(&keys.group_signing, &mut tbs, seam)?;
    // Verify as the backend does, over signed bytes rebuilt from the
    // message's own fields, not over the signing buffer.
    let rebuilt = group_message_tbs(
        &group_id,
        &message.data,
        &message.sender_hmac,
        message.should_push,
    )?;
    verify_publish_signature(&group_id, &rebuilt, &signature[..])
        .map_err(|_| SealError::SelfCheck)?;
    message.signature = signature.to_vec();
    Ok(())
}

/// Seal an encoded `PlaintextCommitLogEntry` for the conversation's commit
/// log, after the self-check.
///
/// The nonce is the first 12 bytes of HMAC-SHA256 under the commit-log nonce
/// key of the topic followed by `plaintext`. The ciphertext is the
/// ChaCha20-Poly1305 encryption of the same `plaintext` under the commit-log
/// encryption key, with the topic as associated data. The same entry sealed
/// twice gives the same bytes, so a retry is a duplicate.
// implements: FORK-074
pub fn seal_commit_log_entry(
    keys: &GroupKeys,
    plaintext: &[u8],
) -> Result<SealedCommitLogEntry, SealError> {
    seal_with(keys, plaintext, &mut NoFault)
}

fn seal_with(
    keys: &GroupKeys,
    plaintext: &[u8],
    seam: &mut impl FaultSeam,
) -> Result<SealedCommitLogEntry, SealError> {
    let commit_log_id = keys.commit_log_id();
    let topic = commit_log_topic(&commit_log_id);
    let nonce = derive_nonce(&keys.commit_log_nonce, &topic, plaintext);
    let ciphertext = aead_encrypt(
        &keys.commit_log_encryption,
        &nonce,
        &topic,
        &seam.encrypt_input(plaintext),
    )
    .map_err(SealError::Encrypt)?;
    let mut tbs = commit_log_entry_tbs(&commit_log_id, &nonce, &ciphertext)?;
    let signature = sign_twice(&keys.commit_log_signing, &mut tbs, seam)?;
    let mut entry = SealedCommitLogEntry {
        commit_log_id: commit_log_id.to_vec(),
        nonce: nonce.to_vec(),
        ciphertext,
        signature: signature.to_vec(),
    };

    // Open the entry as a reader does: verify over signed bytes rebuilt from
    // its own fields, then decrypt. The nonce derived from the decrypted
    // bytes must be the entry's nonce.
    let checked = open_commit_log_entry(keys, &entry).is_ok_and(|opened| {
        derive_nonce(&keys.commit_log_nonce, &topic, &opened)[..] == entry.nonce[..]
    });
    if !checked {
        // A faulty signature and a correct one over the same bytes can reveal
        // the signing key, so the entry's copy is wiped before it is freed.
        drop(Zeroizing::new(std::mem::take(&mut entry.signature)));
        return Err(SealError::SelfCheck);
    }
    Ok(entry)
}

/// Verify and decrypt a sealed commit-log entry read from the conversation's
/// commit-log topic, and return the encoded `PlaintextCommitLogEntry`.
///
/// Every length is checked before any cryptography, then the signature,
/// then the AEAD. A malformed field never reaches the AEAD. The caller
/// decodes the plaintext and checks its `group_id` and `commit_sequence_id`.
// implements: FORK-075
pub fn open_commit_log_entry(
    keys: &GroupKeys,
    entry: &SealedCommitLogEntry,
) -> Result<Vec<u8>, OpenError> {
    let commit_log_id: &[u8; PUBLIC_KEY_BYTES] = entry
        .commit_log_id
        .as_slice()
        .try_into()
        .map_err(|_| OpenError::Malformed(SealedField::CommitLogId))?;
    if *commit_log_id != keys.commit_log_id() {
        return Err(OpenError::OtherCommitLog);
    }
    let nonce: &[u8; COMMIT_LOG_NONCE_BYTES] = entry
        .nonce
        .as_slice()
        .try_into()
        .map_err(|_| OpenError::Malformed(SealedField::Nonce))?;
    if entry.signature.len() != PUBLISH_SIGNATURE_BYTES {
        return Err(OpenError::Malformed(SealedField::Signature));
    }
    if entry.ciphertext.len() < COMMIT_LOG_TAG_BYTES {
        return Err(OpenError::Malformed(SealedField::Ciphertext));
    }

    let tbs = commit_log_entry_tbs(commit_log_id, nonce, &entry.ciphertext)?;
    verify_publish_signature(commit_log_id, &tbs, &entry.signature)?;
    let topic = commit_log_topic(commit_log_id);
    aead_decrypt(
        &keys.commit_log_encryption,
        nonce,
        &topic,
        &entry.ciphertext,
    )
    .map_err(|_| OpenError::Decrypt)
}

/// The first 12 bytes of HMAC-SHA256(nonce key, topic || plaintext). The
/// topic is always 33 bytes, so the concatenation is unambiguous.
fn derive_nonce(
    nonce_key: &[u8; DERIVED_KEY_BYTES],
    topic: &[u8; KEYED_TOPIC_BYTES],
    plaintext: &[u8],
) -> [u8; COMMIT_LOG_NONCE_BYTES] {
    // A stack local: hmac does not wipe its keyed state.
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&nonce_key[..])
        .expect("HMAC-SHA256 takes a key of any length");
    mac.update(topic);
    mac.update(plaintext);
    let tag = mac.finalize().into_bytes();
    let mut nonce = [0; COMMIT_LOG_NONCE_BYTES];
    nonce.copy_from_slice(&tag[..COMMIT_LOG_NONCE_BYTES]);
    nonce
}

/// ChaCha20-Poly1305 through the libcrux provider, which returns an error
/// rather than panicking on a bad length. The types fix every length.
fn aead_encrypt(
    key: &[u8; DERIVED_KEY_BYTES],
    nonce: &[u8; COMMIT_LOG_NONCE_BYTES],
    topic: &[u8; KEYED_TOPIC_BYTES],
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    LIBCRUX_CRYPTO_PROVIDER.aead_encrypt(COMMIT_LOG_AEAD, &key[..], plaintext, nonce, topic)
}

fn aead_decrypt(
    key: &[u8; DERIVED_KEY_BYTES],
    nonce: &[u8; COMMIT_LOG_NONCE_BYTES],
    topic: &[u8; KEYED_TOPIC_BYTES],
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    LIBCRUX_CRYPTO_PROVIDER.aead_decrypt(COMMIT_LOG_AEAD, &key[..], ciphertext, nonce, topic)
}

/// Sign `tbs` twice and require equal bytes. Signing is deterministic, so a
/// difference is a fault. Both signatures are wiped, so a faulty one never
/// outlives this call. A caller that copies the result before its own check
/// wipes that copy when the check fails.
fn sign_twice(
    key: &SigningKey,
    tbs: &mut [u8],
    seam: &mut impl FaultSeam,
) -> Result<Zeroizing<[u8; PUBLISH_SIGNATURE_BYTES]>, SealError> {
    let first = Zeroizing::new(seam.sign(key, tbs, SignPass::First));
    let second = Zeroizing::new(seam.sign(key, tbs, SignPass::Second));
    if *first != *second {
        return Err(SealError::SelfCheck);
    }
    Ok(first)
}

/// Which of the two signings of [`sign_twice`] runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SignPass {
    First,
    Second,
}

/// The points in signing and sealing where a fault can strike. Production
/// code uses [`NoFault`]. Tests inject faults here to prove that the
/// self-check refuses each one.
trait FaultSeam {
    /// The bytes that are encrypted, after the nonce is derived from
    /// `plaintext`.
    fn encrypt_input<'a>(&mut self, plaintext: &'a [u8]) -> Cow<'a, [u8]> {
        Cow::Borrowed(plaintext)
    }

    /// Sign the shared signing buffer.
    fn sign(
        &mut self,
        key: &SigningKey,
        tbs: &mut [u8],
        _pass: SignPass,
    ) -> [u8; PUBLISH_SIGNATURE_BYTES] {
        key.sign(tbs).to_bytes()
    }
}

/// The production seam: no faults.
struct NoFault;

impl FaultSeam for NoFault {}

/// A publish signature fails API verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PublishSignatureError {
    /// The signature is not 64 bytes: a malformed payload.
    #[error("a publish signature is 64 bytes")]
    SignatureLength,
    /// The public key is not the canonical encoding of a curve point.
    #[error("the public key is not the canonical encoding of a curve point")]
    NonCanonicalKey,
    /// The signature does not verify, or uses a small-order key or `R`, a
    /// non-canonical `R`, or an `S` not less than the group order.
    #[error("the publish signature does not verify")]
    Invalid,
}

impl RetryableError for PublishSignatureError {
    fn is_retryable(&self) -> bool {
        false
    }
}

/// Signing or sealing failed, and nothing was released.
#[derive(Debug, thiserror::Error)]
pub enum SealError {
    /// The self-check failed: the two signings differed, the signature did
    /// not verify over signed bytes rebuilt from the envelope, or a sealed
    /// entry did not open to its own nonce. This is a computation fault. It
    /// carries no signature, nonce, ciphertext, or key bytes.
    #[error("the publish self-check failed; nothing was released")]
    SelfCheck,
    /// The commit-log AEAD failed.
    #[error(transparent)]
    Encrypt(CryptoError),
    /// A field is 2^30 bytes or longer.
    #[error(transparent)]
    Encode(#[from] tls_codec::Error),
}

impl RetryableError for SealError {
    fn is_retryable(&self) -> bool {
        false
    }
}

/// A sealed commit-log entry is discarded.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    /// A field has the wrong length.
    #[error("the sealed commit-log entry's {0} has the wrong length")]
    Malformed(SealedField),
    /// The entry names another conversation's commit log.
    #[error("the sealed commit-log entry names another commit log")]
    OtherCommitLog,
    /// The signature fails verification.
    #[error(transparent)]
    Signature(#[from] PublishSignatureError),
    /// The ciphertext does not decrypt.
    #[error("the sealed commit-log entry does not decrypt")]
    Decrypt,
    /// The ciphertext is 2^30 bytes or longer.
    #[error(transparent)]
    Encode(#[from] tls_codec::Error),
}

impl RetryableError for OpenError {
    fn is_retryable(&self) -> bool {
        false
    }
}

/// A field of a sealed commit-log entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SealedField {
    CommitLogId,
    Nonce,
    Ciphertext,
    Signature,
}

impl fmt::Display for SealedField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::CommitLogId => "commit_log_id",
            Self::Nonce => "nonce",
            Self::Ciphertext => "ciphertext",
            Self::Signature => "signature",
        })
    }
}

#[cfg(test)]
mod tests;
