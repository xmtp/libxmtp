//! The vectors in `fixtures/vectors.txt` come from an implementation
//! independent of this module. See `fixtures/README.md`.

use curve25519_dalek::{
    constants::EIGHT_TORSION,
    edwards::EdwardsPoint,
    scalar::{Scalar, clamp_integer},
};
use ed25519_dalek::Verifier as _;
use prost::Message as _;
use sha2::{Digest as _, Sha512};
use xmtp_proto::xmtp::{
    backend::v1::{ClientEnvelope, client_envelope::Payload},
    mls::message_contents::{CommitResult, PlaintextCommitLogEntry},
};

use super::*;

const VECTORS: &str = include_str!("fixtures/vectors.txt");

/// An `r` that differs from the deterministic one for every vector input.
const ANOTHER_R: u64 = 7;

/// The group order L = 2^252 + 27742317777372353535851937790883648493,
/// little-endian.
const GROUP_ORDER: [u8; 32] = [
    0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde, 0x14,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10,
];

fn vector(name: &str) -> Vec<u8> {
    VECTORS
        .lines()
        .filter(|line| !line.starts_with('#'))
        .find_map(|line| {
            let (key, value) = line.split_once(" = ")?;
            (key == name).then(|| hex::decode(value).expect("a vector is hex"))
        })
        .unwrap_or_else(|| panic!("no vector named {name}"))
}

fn vector_array<const N: usize>(name: &str) -> [u8; N] {
    vector(name)
        .try_into()
        .unwrap_or_else(|bytes: Vec<u8>| panic!("{name} is {} bytes, not {N}", bytes.len()))
}

fn vector_u64(name: &str) -> u64 {
    u64::from_be_bytes(vector_array(name))
}

/// The keys of the vector root.
fn vector_keys() -> GroupKeys {
    GroupKeys::derive(&GroupRoot::from_bytes(&vector("root")).unwrap())
}

/// The keys of another group.
fn other_keys() -> GroupKeys {
    GroupKeys::derive(&GroupRoot::from_bytes(&[0x42; GROUP_ROOT_BYTES]).unwrap())
}

/// The entry that the vector plaintext encodes.
fn vector_entry() -> PlaintextCommitLogEntry {
    PlaintextCommitLogEntry {
        group_id: vector("group_id"),
        commit_sequence_id: vector_u64("entry_commit_sequence_id"),
        last_epoch_authenticator: vector("entry_last_epoch_authenticator"),
        commit_result: CommitResult::Applied as i32,
        applied_epoch_number: vector_u64("entry_applied_epoch_number"),
        applied_epoch_authenticator: vector("entry_applied_epoch_authenticator"),
        ..Default::default()
    }
}

/// The vector group message, unsigned.
fn vector_message() -> GroupMessage {
    GroupMessage {
        data: vector("group_message_data"),
        sender_hmac: vector("group_message_sender_hmac"),
        should_push: true,
        signature: vec![],
    }
}

fn signed_message(keys: &GroupKeys) -> GroupMessage {
    let mut message = vector_message();
    sign_group_message(keys, &mut message).unwrap();
    message
}

/// Verify a group message's signature as the backend does.
fn verify_message(
    group_id: &[u8; PUBLIC_KEY_BYTES],
    message: &GroupMessage,
) -> Result<(), PublishSignatureError> {
    let tbs = group_message_tbs(
        group_id,
        &message.data,
        &message.sender_hmac,
        message.should_push,
    )
    .unwrap();
    verify_publish_signature(group_id, &tbs, &message.signature)
}

/// An entry with any field lengths, validly signed over its own fields by
/// the commit-log signing key, so that only a length check can refuse it.
fn signed_entry(
    keys: &GroupKeys,
    commit_log_id: &[u8],
    nonce: &[u8],
    ciphertext: Vec<u8>,
) -> SealedCommitLogEntry {
    let mut topic = vec![TopicKind::CommitLogEntriesV1 as u8];
    topic.extend_from_slice(commit_log_id);
    let tbs = tls_vectors(&[COMMIT_LOG_ENTRY_LABEL, &topic, nonce, &ciphertext], 0).unwrap();
    SealedCommitLogEntry {
        commit_log_id: commit_log_id.to_vec(),
        nonce: nonce.to_vec(),
        signature: keys.commit_log_signing.sign(&tbs).to_bytes().to_vec(),
        ciphertext,
    }
}

fn envelope_bytes(entry: SealedCommitLogEntry) -> Vec<u8> {
    ClientEnvelope {
        payload: Some(Payload::SealedCommitLogEntry(entry)),
    }
    .encode_to_vec()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn wide_hash(parts: &[&[u8]]) -> Scalar {
    let mut hash = Sha512::new();
    for part in parts {
        hash.update(part);
    }
    let mut wide = [0; 64];
    wide.copy_from_slice(&hash.finalize());
    Scalar::from_bytes_mod_order_wide(&wide)
}

/// RFC 8032 Ed25519 signing written out, so that a test can choose `r` or
/// the public key that is hashed. With neither changed it is dalek's
/// signature.
fn raw_sign(
    seed: &[u8; 32],
    hashed_public_key: &[u8; PUBLIC_KEY_BYTES],
    r: Option<Scalar>,
    message: &[u8],
) -> [u8; PUBLISH_SIGNATURE_BYTES] {
    let expanded = Sha512::digest(seed);
    let mut lower = [0; 32];
    lower.copy_from_slice(&expanded[..32]);
    let a = Scalar::from_bytes_mod_order(clamp_integer(lower));
    let r = r.unwrap_or_else(|| wide_hash(&[&expanded[32..], message]));
    let big_r = EdwardsPoint::mul_base(&r).compress();
    let k = wide_hash(&[big_r.as_bytes(), hashed_public_key, message]);
    let s = r + k * a;
    let mut signature = [0; PUBLISH_SIGNATURE_BYTES];
    signature[..32].copy_from_slice(big_r.as_bytes());
    signature[32..].copy_from_slice(s.as_bytes());
    signature
}

/// A fault the test seam injects.
#[derive(Debug, Clone, Copy)]
enum Fault {
    None,
    /// The second signing returns a valid signature with another `r`.
    SecondSignatureWithAnotherR,
    /// Both signings return the signature with the low bit of `S` flipped.
    FlippedSBit,
    /// Both signings hash this public key instead of the key's own.
    HashedPublicKey([u8; PUBLIC_KEY_BYTES]),
    /// The last bit of the signing buffer flips before the first signing, so
    /// both signings agree on the wrong bytes. The envelope stays intact.
    FlippedTbsBit,
    /// One plaintext bit flips after the nonce is derived from it.
    FlippedPlaintextBit,
}

struct Injected(Fault);

impl FaultSeam for Injected {
    fn encrypt_input<'a>(&mut self, plaintext: &'a [u8]) -> Cow<'a, [u8]> {
        match self.0 {
            Fault::FlippedPlaintextBit => {
                let mut flipped = plaintext.to_vec();
                flipped[plaintext.len() / 2] ^= 1;
                Cow::Owned(flipped)
            }
            _ => Cow::Borrowed(plaintext),
        }
    }

    fn sign(
        &mut self,
        key: &SigningKey,
        tbs: &mut [u8],
        pass: SignPass,
    ) -> [u8; PUBLISH_SIGNATURE_BYTES] {
        let public_key = key.verifying_key().to_bytes();
        match (self.0, pass) {
            (Fault::SecondSignatureWithAnotherR, SignPass::Second) => raw_sign(
                key.as_bytes(),
                &public_key,
                Some(Scalar::from(ANOTHER_R)),
                tbs,
            ),
            (Fault::FlippedSBit, _) => {
                let mut signature = key.sign(tbs).to_bytes();
                signature[32] ^= 1;
                signature
            }
            (Fault::HashedPublicKey(hashed), _) => raw_sign(key.as_bytes(), &hashed, None, tbs),
            (Fault::FlippedTbsBit, SignPass::First) => {
                let last = tbs.len() - 1;
                tbs[last] ^= 1;
                key.sign(tbs).to_bytes()
            }
            _ => key.sign(tbs).to_bytes(),
        }
    }
}

fn seal_with_fault(fault: Fault) -> Result<SealedCommitLogEntry, SealError> {
    seal_with(&vector_keys(), &vector("plaintext"), &mut Injected(fault))
}

/// The SHA-256 states the group-key HMAC keys are wiped on drop: sha2's
/// `zeroize` feature is on. A build without it does not compile this test.
#[xmtp_common::test]
fn hmac_hash_states_are_wiped_on_drop() {
    fn wiped_on_drop<T: sha2_zeroize::digest::zeroize::ZeroizeOnDrop>() {}
    wiped_on_drop::<sha2_zeroize::Sha256>();
}

// verifies: GMOD-041
#[xmtp_common::test(unwrap_try = true)]
fn vectors_pin_the_key_derivation() {
    let root = vector("root");
    assert_eq!(
        extract(&GroupRoot::from_bytes(&root)?)[..],
        vector("prk")[..]
    );

    let keys = GroupKeys::derive(&GroupRoot::from_bytes(&root)?);
    assert_eq!(
        keys.group_signing.as_bytes()[..],
        vector("group_signing_seed")[..]
    );
    assert_eq!(
        keys.commit_log_signing.as_bytes()[..],
        vector("commit_log_signing_seed")[..]
    );
    assert_eq!(
        keys.commit_log_encryption[..],
        vector("commit_log_encryption_key")[..]
    );
    assert_eq!(
        keys.commit_log_nonce[..],
        vector("commit_log_nonce_key")[..]
    );
    assert_eq!(keys.group_id(), vector_array("group_id"));
    assert_eq!(keys.commit_log_id(), vector_array("commit_log_id"));
}

// verifies: FORK-074
#[xmtp_common::test(unwrap_try = true)]
fn vectors_pin_a_sealed_entry() {
    let keys = vector_keys();
    let plaintext = vector_entry().encode_to_vec();
    assert_eq!(plaintext, vector("plaintext"));
    assert_eq!(
        commit_log_topic(&keys.commit_log_id()),
        vector_array("commit_log_topic")
    );
    assert_eq!(
        commit_log_entry_tbs(
            &keys.commit_log_id(),
            &vector_array("nonce"),
            &vector("ciphertext")
        )?,
        vector("commit_log_entry_tbs")
    );

    let entry = seal_commit_log_entry(&keys, &plaintext)?;
    assert_eq!(entry.commit_log_id, vector("commit_log_id"));
    assert_eq!(entry.nonce, vector("nonce"));
    assert_eq!(entry.ciphertext, vector("ciphertext"));
    assert_eq!(entry.signature, vector("commit_log_entry_signature"));
}

#[xmtp_common::test(unwrap_try = true)]
fn vectors_pin_a_group_message_signature() {
    let keys = vector_keys();
    let mut message = vector_message();
    assert_eq!(
        group_message_tbs(
            &keys.group_id(),
            &message.data,
            &message.sender_hmac,
            message.should_push
        )?,
        vector("group_message_tbs")
    );
    sign_group_message(&keys, &mut message)?;
    assert_eq!(message.signature, vector("group_message_signature"));
}

#[xmtp_common::test(unwrap_try = true)]
fn dalek_and_libcrux_agree_on_keys_and_signatures() {
    let keys = vector_keys();
    let cases = [
        (
            &keys.group_signing,
            keys.group_id(),
            vector("group_message_tbs"),
        ),
        (
            &keys.commit_log_signing,
            keys.commit_log_id(),
            vector("commit_log_entry_tbs"),
        ),
    ];
    for (key, id, tbs) in cases {
        let mut public_key = [0; PUBLIC_KEY_BYTES];
        libcrux_ed25519::secret_to_public(&mut public_key, key.as_bytes());
        assert_eq!(public_key, id);
        assert_eq!(
            libcrux_ed25519::sign(&tbs, key.as_bytes())?,
            key.sign(&tbs).to_bytes()
        );
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn the_test_signer_matches_dalek() {
    let keys = vector_keys();
    let tbs = vector("commit_log_entry_tbs");
    assert_eq!(
        raw_sign(
            keys.commit_log_signing.as_bytes(),
            &keys.commit_log_id(),
            None,
            &tbs
        ),
        keys.commit_log_signing.sign(&tbs).to_bytes()
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn every_derived_key_differs() {
    let keys = vector_keys();
    let group_id = keys.group_id();
    let commit_log_id = keys.commit_log_id();
    let all: [&[u8]; 6] = [
        keys.group_signing.as_bytes(),
        keys.commit_log_signing.as_bytes(),
        &keys.commit_log_encryption[..],
        &keys.commit_log_nonce[..],
        &group_id,
        &commit_log_id,
    ];
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            assert_ne!(a, b);
        }
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn generated_roots_differ() {
    let a = GroupKeys::derive(&GroupRoot::generate());
    let b = GroupKeys::derive(&GroupRoot::generate());
    assert_ne!(a.group_id(), b.group_id());
}

#[xmtp_common::test(unwrap_try = true)]
fn a_root_is_32_bytes() {
    for len in [0, 31, 33] {
        assert_eq!(
            GroupRoot::from_bytes(&vec![1; len]).unwrap_err(),
            GroupRootLengthError { len }
        );
    }
    assert_eq!(
        GroupRoot::from_bytes(&vector("root"))?.as_bytes()[..],
        vector("root")[..]
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn debug_shows_no_key_bytes() {
    let root = GroupRoot::from_bytes(&vector("root"))?;
    let keys = GroupKeys::derive(&root);
    let text = format!("{root:?} {keys:?} {root:#?} {keys:#?}");
    for name in [
        "root",
        "group_signing_seed",
        "commit_log_signing_seed",
        "commit_log_encryption_key",
        "commit_log_nonce_key",
        "group_id",
        "commit_log_id",
    ] {
        let bytes = vector(name);
        // A derived `Debug` prints bytes as a decimal list.
        let decimal = format!("{:?}", &bytes[..4]);
        let decimal = decimal.trim_start_matches('[').trim_end_matches(']');
        for form in [
            hex::encode(&bytes),
            hex::encode_upper(&bytes),
            decimal.to_owned(),
        ] {
            assert!(!text.contains(&form), "Debug shows {name}: {text}");
        }
    }
}

// verifies: FORK-074
#[xmtp_common::test(unwrap_try = true)]
fn sealing_twice_gives_identical_bytes() {
    let keys = vector_keys();
    let plaintext = vector_entry().encode_to_vec();
    let first = seal_commit_log_entry(&keys, &plaintext)?;
    let second = seal_commit_log_entry(&keys, &plaintext)?;
    assert_eq!(envelope_bytes(first), envelope_bytes(second));
}

// verifies: FORK-077
#[xmtp_common::test(unwrap_try = true)]
fn opening_recovers_the_plaintext() {
    let keys = vector_keys();
    let plaintext = vector_entry().encode_to_vec();
    let entry = seal_commit_log_entry(&keys, &plaintext)?;
    assert_eq!(open_commit_log_entry(&keys, &entry)?, plaintext);
}

// verifies: FORK-074
#[xmtp_common::test(unwrap_try = true)]
fn the_nonce_depends_on_the_topic() {
    let keys = vector_keys();
    let plaintext = vector("plaintext");
    let nonce =
        |topic: &[u8; KEYED_TOPIC_BYTES]| derive_nonce(&keys.commit_log_nonce, topic, &plaintext);
    let own = nonce(&commit_log_topic(&keys.commit_log_id()));
    assert_eq!(own, vector_array("nonce"));
    assert_ne!(own, nonce(&commit_log_topic(&other_keys().commit_log_id())));
    assert_ne!(own, nonce(&group_message_topic(&keys.commit_log_id())));
}

// verifies: FORK-074
#[xmtp_common::test(unwrap_try = true)]
fn entries_that_differ_in_one_field_get_different_nonces() {
    let keys = vector_keys();
    let base = vector_entry();
    let changes: [fn(&mut PlaintextCommitLogEntry); 6] = [
        |entry| entry.group_id[0] ^= 1,
        |entry| entry.commit_sequence_id += 1,
        |entry| entry.last_epoch_authenticator[0] ^= 1,
        |entry| entry.commit_result = CommitResult::WrongEpoch as i32,
        |entry| entry.applied_epoch_number += 1,
        |entry| entry.applied_epoch_authenticator[0] ^= 1,
    ];
    let mut nonces = vec![seal_commit_log_entry(&keys, &base.encode_to_vec())?.nonce];
    for change in changes {
        let mut entry = base.clone();
        change(&mut entry);
        nonces.push(seal_commit_log_entry(&keys, &entry.encode_to_vec())?.nonce);
    }
    for (i, a) in nonces.iter().enumerate() {
        for b in &nonces[i + 1..] {
            assert_ne!(a, b);
        }
    }
}

// verifies: FORK-077
#[xmtp_common::test(unwrap_try = true)]
fn an_entry_sealed_under_other_associated_data_does_not_open() {
    let keys = vector_keys();
    let commit_log_id = keys.commit_log_id();
    let plaintext = vector("plaintext");
    let nonce = vector_array::<COMMIT_LOG_NONCE_BYTES>("nonce");
    let other_topic = commit_log_topic(&other_keys().commit_log_id());
    for aad in [&[][..], &other_topic[..]] {
        let ciphertext = LIBCRUX_CRYPTO_PROVIDER.aead_encrypt(
            COMMIT_LOG_AEAD,
            &keys.commit_log_encryption[..],
            &plaintext,
            &nonce,
            aad,
        )?;
        let entry = signed_entry(&keys, &commit_log_id, &nonce, ciphertext);
        assert!(matches!(
            open_commit_log_entry(&keys, &entry),
            Err(OpenError::Decrypt)
        ));
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn the_sealed_envelope_hides_the_group_id_and_the_plaintext() {
    let keys = vector_keys();
    let group_id = keys.group_id();
    let plaintext = vector_entry().encode_to_vec();
    assert!(contains(&plaintext, &group_id));

    let bytes = envelope_bytes(seal_commit_log_entry(&keys, &plaintext)?);
    assert!(!contains(&bytes, &group_id));
    assert!(!contains(&bytes, &vector("entry_last_epoch_authenticator")));
    assert_ne!(keys.commit_log_id(), group_id);
}

// verifies: FORK-077
#[xmtp_common::test(unwrap_try = true)]
fn open_refuses_malformed_fields_before_any_cryptography() {
    let keys = vector_keys();
    let commit_log_id = keys.commit_log_id();
    let good = seal_commit_log_entry(&keys, &vector("plaintext"))?;

    for len in [0, 11, 13] {
        let entry = signed_entry(
            &keys,
            &commit_log_id,
            &vec![7; len],
            good.ciphertext.clone(),
        );
        assert!(matches!(
            open_commit_log_entry(&keys, &entry),
            Err(OpenError::Malformed(SealedField::Nonce))
        ));
    }
    let entry = signed_entry(&keys, &commit_log_id, &good.nonce, vec![9; 15]);
    assert!(matches!(
        open_commit_log_entry(&keys, &entry),
        Err(OpenError::Malformed(SealedField::Ciphertext))
    ));
    for len in [0, 63, 65] {
        let mut entry = good.clone();
        entry.signature.resize(len, 0);
        assert!(matches!(
            open_commit_log_entry(&keys, &entry),
            Err(OpenError::Malformed(SealedField::Signature))
        ));
    }
    let entry = signed_entry(
        &keys,
        &commit_log_id[..31],
        &good.nonce,
        good.ciphertext.clone(),
    );
    assert!(matches!(
        open_commit_log_entry(&keys, &entry),
        Err(OpenError::Malformed(SealedField::CommitLogId))
    ));
}

/// Lengths come before the signature, and the signature before the AEAD:
/// each entry below fails two checks and must report the earlier one.
// verifies: FORK-077
#[xmtp_common::test(unwrap_try = true)]
fn open_checks_lengths_then_the_signature_then_the_aead() {
    let keys = vector_keys();
    let good = seal_commit_log_entry(&keys, &vector("plaintext"))?;
    let garbage_signature = vec![0x5a; PUBLISH_SIGNATURE_BYTES];

    let mut short_ciphertext = good.clone();
    short_ciphertext
        .ciphertext
        .truncate(COMMIT_LOG_TAG_BYTES - 1);
    short_ciphertext.signature = garbage_signature.clone();
    assert!(matches!(
        open_commit_log_entry(&keys, &short_ciphertext),
        Err(OpenError::Malformed(SealedField::Ciphertext))
    ));

    let mut short_nonce = good.clone();
    short_nonce.nonce.truncate(COMMIT_LOG_NONCE_BYTES - 1);
    short_nonce.signature = garbage_signature;
    assert!(matches!(
        open_commit_log_entry(&keys, &short_nonce),
        Err(OpenError::Malformed(SealedField::Nonce))
    ));

    let mut tampered = good;
    tampered.ciphertext[0] ^= 1;
    assert!(matches!(
        open_commit_log_entry(&keys, &tampered),
        Err(OpenError::Signature(PublishSignatureError::Invalid))
    ));
}

// verifies: FORK-077
#[xmtp_common::test(unwrap_try = true)]
fn open_refuses_an_entry_of_another_commit_log() {
    let entry = seal_commit_log_entry(&other_keys(), &vector("plaintext"))?;
    assert!(matches!(
        open_commit_log_entry(&vector_keys(), &entry),
        Err(OpenError::OtherCommitLog)
    ));
}

// verifies: FORK-077
#[xmtp_common::test(unwrap_try = true)]
fn open_refuses_a_signature_by_another_key() {
    let keys = vector_keys();
    let mut entry = seal_commit_log_entry(&keys, &vector("plaintext"))?;
    let tbs = vector("commit_log_entry_tbs");
    for key in [&keys.group_signing, &other_keys().commit_log_signing] {
        entry.signature = key.sign(&tbs).to_bytes().to_vec();
        assert!(matches!(
            open_commit_log_entry(&keys, &entry),
            Err(OpenError::Signature(PublishSignatureError::Invalid))
        ));
    }
}

// verifies: FORK-077
#[xmtp_common::test(unwrap_try = true)]
fn open_refuses_a_ciphertext_that_does_not_decrypt() {
    let keys = vector_keys();
    let mut ciphertext = vector("ciphertext");
    ciphertext[0] ^= 1;
    let entry = signed_entry(&keys, &keys.commit_log_id(), &vector("nonce"), ciphertext);
    assert!(matches!(
        open_commit_log_entry(&keys, &entry),
        Err(OpenError::Decrypt)
    ));
}

// verifies: API-300
#[xmtp_common::test(unwrap_try = true)]
fn verify_accepts_a_valid_signature_and_refuses_any_change() {
    let keys = vector_keys();
    let group_id = keys.group_id();
    let message = signed_message(&keys);
    assert_eq!(verify_message(&group_id, &message), Ok(()));

    let changes: [fn(&mut GroupMessage); 4] = [
        |message| message.should_push = !message.should_push,
        |message| message.sender_hmac[0] ^= 1,
        |message| message.data.push(0),
        |message| message.data[0] ^= 1,
    ];
    for change in changes {
        let mut changed = message.clone();
        change(&mut changed);
        assert_eq!(
            verify_message(&group_id, &changed),
            Err(PublishSignatureError::Invalid)
        );
    }
    assert_eq!(
        verify_message(&other_keys().group_id(), &message),
        Err(PublishSignatureError::Invalid)
    );
    for len in [0, 63, 65] {
        let mut changed = message.clone();
        changed.signature.resize(len, 0);
        assert_eq!(
            verify_message(&group_id, &changed),
            Err(PublishSignatureError::SignatureLength)
        );
    }
}

// verifies: API-300
#[xmtp_common::test(unwrap_try = true)]
fn verify_refuses_a_signature_over_the_other_structure() {
    let keys = vector_keys();
    let group_id = keys.group_id();
    let message = vector_message();
    let group_tbs = group_message_tbs(
        &group_id,
        &message.data,
        &message.sender_hmac,
        message.should_push,
    )?;

    // The group signing key over a CommitLogEntryTBS of the same bytes.
    let other_tbs = commit_log_entry_tbs(&group_id, &[0; COMMIT_LOG_NONCE_BYTES], &message.data)?;
    let signature = keys.group_signing.sign(&other_tbs).to_bytes();
    assert_eq!(
        verify_publish_signature(&group_id, &group_tbs, &signature),
        Err(PublishSignatureError::Invalid)
    );

    // A sealed entry's signature presented as a group message under the
    // commit-log id.
    let entry = seal_commit_log_entry(&keys, &vector("plaintext"))?;
    let commit_log_id = keys.commit_log_id();
    let as_message = group_message_tbs(&commit_log_id, &entry.ciphertext, &[], false)?;
    assert_eq!(
        verify_publish_signature(&commit_log_id, &as_message, &entry.signature),
        Err(PublishSignatureError::Invalid)
    );
}

/// A small-order key `A` lets anyone sign: with `R = [S]B`, plain `verify`
/// accepts every message whose `k` sends `[k]A` to the identity.
// verifies: API-300
#[xmtp_common::test(unwrap_try = true)]
fn verify_refuses_every_small_order_key() {
    let s = Scalar::from(0x5eed_u64);
    let mut signature = [0; PUBLISH_SIGNATURE_BYTES];
    signature[..32].copy_from_slice(EdwardsPoint::mul_base(&s).compress().as_bytes());
    signature[32..].copy_from_slice(s.as_bytes());
    let parsed = Signature::from_bytes(&signature);

    for point in EIGHT_TORSION {
        let key = point.compress().to_bytes();
        let plain = VerifyingKey::from_bytes(&key)?;
        let message = (0_u32..256)
            .map(u32::to_le_bytes)
            .find(|message| plain.verify(message, &parsed).is_ok())
            .expect("plain verify accepts a forgery for some message");
        assert_eq!(
            verify_publish_signature(&key, &message, &signature),
            Err(PublishSignatureError::Invalid)
        );
    }
}

/// `S + L` is a second encoding of a valid `S`. Every dalek path refuses it
/// unless `legacy_compatibility` is on.
// verifies: API-300
#[xmtp_common::test(unwrap_try = true)]
fn verify_refuses_s_plus_the_group_order() {
    let keys = vector_keys();
    let group_id = keys.group_id();
    let message = signed_message(&keys);
    let tbs = vector("group_message_tbs");
    let mut signature: [u8; PUBLISH_SIGNATURE_BYTES] = message.signature.as_slice().try_into()?;
    assert_eq!(Scalar::from_bytes_mod_order(GROUP_ORDER), Scalar::ZERO);
    // S < L < 2^253, so the little-endian sum cannot overflow 32 bytes.
    let mut carry = 0_u16;
    for (s, l) in signature[32..].iter_mut().zip(GROUP_ORDER) {
        let sum = u16::from(*s) + u16::from(l) + carry;
        *s = sum.to_le_bytes()[0];
        carry = sum >> 8;
    }
    assert_eq!(carry, 0);
    assert_eq!(
        verify_publish_signature(&group_id, &tbs, &signature),
        Err(PublishSignatureError::Invalid)
    );
}

// verifies: API-300
#[xmtp_common::test(unwrap_try = true)]
fn verify_refuses_a_non_canonical_key_encoding() {
    let signature = signed_message(&vector_keys()).signature;
    let tbs = vector("group_message_tbs");

    // y = p + 3: it decodes to the point with y = 3, which is not of small
    // order, and re-encodes to other bytes.
    let mut key = [0xff; PUBLIC_KEY_BYTES];
    key[0] = 0xf0;
    key[31] = 0x7f;
    let point = CompressedEdwardsY(key)
        .decompress()
        .expect("y = 3 is on the curve");
    assert!(!point.is_small_order());
    assert_ne!(point.compress().to_bytes(), key);
    // A round trip through `VerifyingKey` would pass it.
    assert_eq!(VerifyingKey::from_bytes(&key)?.to_bytes(), key);
    assert_eq!(
        verify_publish_signature(&key, &tbs, &signature),
        Err(PublishSignatureError::NonCanonicalKey)
    );

    // x = 0 with the sign bit set: a second encoding of the identity.
    let mut negative_zero = [0; PUBLIC_KEY_BYTES];
    negative_zero[0] = 1;
    negative_zero[31] = 0x80;
    assert_eq!(
        verify_publish_signature(&negative_zero, &tbs, &signature),
        Err(PublishSignatureError::NonCanonicalKey)
    );
}

#[xmtp_common::test(unwrap_try = true)]
fn the_unfaulted_seam_seals_and_signs() {
    assert_eq!(
        seal_with_fault(Fault::None)?,
        seal_commit_log_entry(&vector_keys(), &vector("plaintext"))?
    );
    let mut message = vector_message();
    sign_group_message_with(&vector_keys(), &mut message, &mut Injected(Fault::None))?;
    assert_eq!(message.signature, vector("group_message_signature"));
}

/// The faulty second signature is valid on its own, so only signing twice
/// refuses it.
#[xmtp_common::test(unwrap_try = true)]
fn seal_self_check_refuses_a_second_signature_with_another_r() {
    let keys = vector_keys();
    let tbs = vector("commit_log_entry_tbs");
    let faulty = raw_sign(
        keys.commit_log_signing.as_bytes(),
        &keys.commit_log_id(),
        Some(Scalar::from(ANOTHER_R)),
        &tbs,
    );
    assert_eq!(
        verify_publish_signature(&keys.commit_log_id(), &tbs, &faulty),
        Ok(())
    );
    assert_ne!(faulty.to_vec(), vector("commit_log_entry_signature"));

    assert!(matches!(
        seal_with_fault(Fault::SecondSignatureWithAnotherR),
        Err(SealError::SelfCheck)
    ));
}

#[xmtp_common::test(unwrap_try = true)]
fn seal_self_check_refuses_a_flipped_s_bit() {
    assert!(matches!(
        seal_with_fault(Fault::FlippedSBit),
        Err(SealError::SelfCheck)
    ));
}

/// Signing with the group id as the public key: the "double public key"
/// fault. Both signings agree, so only verification refuses it.
#[xmtp_common::test(unwrap_try = true)]
fn seal_self_check_refuses_a_signature_under_the_group_id() {
    let group_id = vector_keys().group_id();
    assert!(matches!(
        seal_with_fault(Fault::HashedPublicKey(group_id)),
        Err(SealError::SelfCheck)
    ));
}

/// Both signings cover the corrupted signing buffer and agree, and the
/// signature verifies over that buffer. Only verification over signed bytes
/// rebuilt from the envelope refuses it.
#[xmtp_common::test(unwrap_try = true)]
fn seal_self_check_refuses_a_fault_in_the_signing_buffer() {
    assert!(matches!(
        seal_with_fault(Fault::FlippedTbsBit),
        Err(SealError::SelfCheck)
    ));
}

/// The ciphertext holds other bytes than the nonce was derived from. It
/// decrypts and its signature verifies, so only re-deriving the nonce
/// refuses it.
#[xmtp_common::test(unwrap_try = true)]
fn seal_self_check_refuses_a_plaintext_fault_after_the_nonce() {
    assert!(matches!(
        seal_with_fault(Fault::FlippedPlaintextBit),
        Err(SealError::SelfCheck)
    ));
}

#[xmtp_common::test(unwrap_try = true)]
fn seal_error_shows_no_signature_bytes() {
    let honest = vector("commit_log_entry_signature");
    let mut faulty = honest.clone();
    faulty[32] ^= 1;
    let error = seal_with_fault(Fault::FlippedSBit).unwrap_err();
    let text = format!("{error:?} {error:#?} {error}");
    for signature in [&honest, &faulty] {
        let decimal = format!("{:?}", &signature[..4]);
        let decimal = decimal.trim_start_matches('[').trim_end_matches(']');
        for form in [
            hex::encode(signature),
            hex::encode_upper(signature),
            hex::encode(&signature[..16]),
            decimal.to_owned(),
        ] {
            assert!(!text.contains(&form), "SealError shows a signature: {text}");
        }
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn group_message_self_check_refuses_each_fault() {
    let keys = vector_keys();
    for fault in [
        Fault::SecondSignatureWithAnotherR,
        Fault::FlippedSBit,
        Fault::HashedPublicKey(keys.commit_log_id()),
        Fault::FlippedTbsBit,
    ] {
        let mut message = vector_message();
        let result = sign_group_message_with(&keys, &mut message, &mut Injected(fault));
        assert!(
            matches!(result, Err(SealError::SelfCheck)),
            "{fault:?} gave {result:?}"
        );
        assert!(message.signature.is_empty(), "{fault:?} set a signature");
    }
}
