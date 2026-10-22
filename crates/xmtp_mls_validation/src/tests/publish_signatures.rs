//! Publish signatures at admission: routing by a public key, the lengths a
//! signed payload needs, and the shared verification rule over each signed
//! structure.

use super::*;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hkdf::Hkdf;
use sha2::Sha256;
use xmtp_mls_common::group_keys::{
    PublishSignatureError, SealedField, commit_log_entry_tbs, group_message_tbs,
    verify_publish_signature,
};
use xmtp_proto::xmtp::backend::v1::{GroupMessage, SealedCommitLogEntry};

/// The encoding of the base point B.
const BASE_POINT: &str = "5866666666666666666666666666666666666666666666666666666666666666";
/// The group order L, little-endian.
const GROUP_ORDER: &str = "edd3f55c1a631258d69cf7a2def9de1400000000000000000000000000000010";

fn bytes<const N: usize>(hex: &str) -> [u8; N] {
    hex::decode(hex)
        .expect("test hex decodes")
        .try_into()
        .expect("test hex has the right length")
}

fn group_message(envelope: &mut ClientEnvelope) -> &mut GroupMessage {
    match envelope.payload.as_mut() {
        Some(Payload::GroupMessage(message)) => message,
        _ => unreachable!("the fixture is a group message"),
    }
}

fn sealed_entry(envelope: &mut ClientEnvelope) -> &mut SealedCommitLogEntry {
    match envelope.payload.as_mut() {
        Some(Payload::SealedCommitLogEntry(entry)) => entry,
        _ => unreachable!("the fixture is a sealed commit-log entry"),
    }
}

/// Parse and verify, as admission does for an envelope that is not a
/// duplicate.
fn admit(envelope: ClientEnvelope) -> Result<ParsedEnvelope, ValidationError> {
    let parsed = parse_envelope(envelope)?;
    verify_publish(&parsed)?;
    Ok(parsed)
}

/// Admission must refuse `envelope` because its signature does not verify.
#[track_caller]
fn assert_invalid_signature(envelope: ClientEnvelope, case: &str) {
    let parsed = parse_envelope(envelope).unwrap_or_else(|error| panic!("{case}: {error}"));
    let error = verify_publish(&parsed)
        .err()
        .unwrap_or_else(|| panic!("{case}: the signature must not verify"));
    assert!(
        matches!(
            error,
            ValidationError::PublishSignature(PublishSignatureError::Invalid)
        ),
        "{case}: {error:?}"
    );
    assert_eq!(error.reason(), Reason::InvalidSignature, "{case}");
    assert!(!error.is_retryable(), "{case}");
}

/// Parsing must refuse `envelope` as malformed with `expected`.
#[track_caller]
fn assert_malformed(envelope: ClientEnvelope, expected: fn(&ValidationError) -> bool, case: &str) {
    let error = parse_envelope(envelope)
        .err()
        .unwrap_or_else(|| panic!("{case}: parsing must fail"));
    assert!(expected(&error), "{case}: {error:?}");
    assert_eq!(error.reason(), Reason::MalformedPayload, "{case}");
    assert!(!error.is_retryable(), "{case}");
}

/// The group signing key of `group_keys(seed)`, derived here from the GMOD
/// salt and label rather than through `GroupKeys`, so a test can sign bytes
/// that `GroupKeys` never signs.
fn raw_group_signing_key(seed: u8) -> SigningKey {
    let hkdf = Hkdf::<Sha256>::new(Some(b"xmtp group root"), &[seed; 32]);
    let mut key = [0; 32];
    hkdf.expand(b"xmtp group signing key", &mut key)
        .expect("32 bytes is a valid HKDF-SHA256 output length");
    let key = SigningKey::from_bytes(&key);
    assert_eq!(key.verifying_key().to_bytes(), group_keys(seed).group_id());
    key
}

/// `S + L` as a 32-byte little-endian integer: the same scalar modulo L, in a
/// second, non-canonical encoding.
fn plus_group_order(scalar: &[u8]) -> Vec<u8> {
    let order: [u8; 32] = bytes(GROUP_ORDER);
    let mut carry = 0u16;
    let sum: Vec<u8> = scalar
        .iter()
        .zip(order)
        .map(|(a, b)| {
            let total = u16::from(*a) + u16::from(b) + carry;
            carry = total >> 8;
            total as u8
        })
        .collect();
    assert_eq!(carry, 0, "S + L fits in 32 bytes");
    sum
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: TOPIC-001, API-230, API-300
fn keyed_group_messages_route_by_their_group_id_and_verify() {
    let keys = group_keys(1);
    for (kind, flag) in [
        (GroupMessageKind::Application, false),
        (GroupMessageKind::Proposal, true),
        (GroupMessageKind::Commit, true),
    ] {
        let envelope = keyed_group_message_envelope(&keys, kind, [0xaa]);
        let parsed = parse_envelope(envelope.clone())?;
        assert_eq!(
            parsed.topic.cloned_vec(),
            expected_topic(TopicKind::GroupMessagesV1, keys.group_id())
        );
        assert_eq!(parsed.topic.cloned_vec().len(), 33);
        assert!(parsed.is_signed());
        assert_eq!(parsed.is_commit_or_proposal, flag);
        assert_eq!(parsed.envelope, envelope);
        verify_publish(&parsed)?;
    }
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: TOPIC-001, API-230, API-300
fn sealed_commit_log_entries_route_by_their_commit_log_id_and_verify() {
    let keys = group_keys(1);
    let envelope = sealed_commit_log_envelope(&keys, 7);
    let parsed = parse_envelope(envelope.clone())?;
    assert_eq!(
        parsed.topic.cloned_vec(),
        expected_topic(TopicKind::CommitLogEntriesV1, keys.commit_log_id())
    );
    assert_ne!(keys.commit_log_id(), keys.group_id());
    assert!(parsed.is_signed());
    assert!(!parsed.is_commit_or_proposal);
    assert_eq!(parsed.push_fields(), (false, None));
    assert_eq!(parsed.envelope, envelope);
    verify_publish(&parsed)?;
}

/// Until the cutover, a 16-byte legacy group id still routes and carries no
/// signature, because no installation holds a key for it. Its `signature`
/// field is not read.
#[xmtp_common::test(unwrap_try = true)]
// verifies: TOPIC-001
fn legacy_group_payloads_stay_unsigned_until_the_cutover() {
    let mut message = group_message_envelope(GROUP_ID, GroupMessageKind::Application, []);
    let parsed = admit(message.clone())?;
    assert!(!parsed.is_signed());
    group_message(&mut message).signature = vec![0x55; 64];
    assert!(!admit(message)?.is_signed());
    let parsed = admit(commit_log_envelope(GROUP_ID))?;
    assert!(!parsed.is_signed());
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: API-230
fn a_keyed_group_message_needs_a_64_byte_signature() {
    let keys = group_keys(1);
    for length in [0, 63, 65] {
        let mut envelope = keyed_group_message_envelope(&keys, GroupMessageKind::Application, []);
        group_message(&mut envelope).signature.resize(length, 0x55);
        assert_malformed(
            envelope,
            |error| {
                matches!(
                    error,
                    ValidationError::PublishSignature(PublishSignatureError::SignatureLength)
                )
            },
            &format!("{length}-byte signature"),
        );
    }
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: API-230, API-300
fn any_change_to_a_signed_group_message_is_refused() {
    let keys = group_keys(1);
    let signed = keyed_group_message_envelope(&keys, GroupMessageKind::Commit, []);
    admit(signed.clone())?;
    type Change = fn(&mut GroupMessage);
    let cases: [(&str, Change); 7] = [
        ("flipped should_push", |message| {
            message.should_push = !message.should_push
        }),
        ("changed sender_hmac", |message| message.sender_hmac[0] ^= 1),
        ("cleared sender_hmac", |message| message.sender_hmac.clear()),
        ("one appended data byte", |message| message.data.push(0)),
        ("flipped last data byte", |message| {
            *message.data.last_mut().expect("data is not empty") ^= 1
        }),
        ("flipped bit of R", |message| message.signature[0] ^= 1),
        ("flipped bit of S", |message| message.signature[40] ^= 1),
    ];
    for (case, change) in cases {
        let mut envelope = signed.clone();
        change(group_message(&mut envelope));
        assert_invalid_signature(envelope, case);
    }
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: API-300
fn a_signature_under_another_key_or_structure_is_refused() {
    let keys = group_keys(1);
    let signed = keyed_group_message_envelope(&keys, GroupMessageKind::Application, []);

    // Signed by another group's key over that group's topic.
    let mut envelope = signed.clone();
    let message = group_message(&mut envelope);
    xmtp_mls_common::group_keys::sign_group_message(&group_keys(2), message)?;
    assert_invalid_signature(envelope, "another group's key");

    // The group signing key over a `CommitLogEntryTBS`.
    let mut envelope = signed.clone();
    let tbs = commit_log_entry_tbs(&keys.group_id(), &[0; 12], &[0; 16])?;
    group_message(&mut envelope).signature =
        raw_group_signing_key(1).sign(&tbs).to_bytes().to_vec();
    assert_invalid_signature(envelope, "a signature over CommitLogEntryTBS");

    // The group's own commit-log entry signature.
    let mut envelope = signed;
    let mut entry = sealed_commit_log_envelope(&keys, 7);
    group_message(&mut envelope).signature = sealed_entry(&mut entry).signature.clone();
    assert_invalid_signature(envelope, "a commit-log entry signature");
}

/// Three signatures that plain `verify` accepts and the shared rule refuses.
#[xmtp_common::test(unwrap_try = true)]
// verifies: API-300
fn signatures_that_plain_verify_accepts_are_refused() {
    // (i) The identity point as the group id. [k]A is the identity, so
    // R = [S]B verifies for any message; here S = 1 and R = B.
    let identity: [u8; 32] = bytes(&format!("01{}", "00".repeat(31)));
    let mut envelope = group_message_envelope(identity, GroupMessageKind::Application, []);
    let message = group_message(&mut envelope);
    let mut forged = bytes::<32>(BASE_POINT).to_vec();
    forged.extend(bytes::<32>(&format!("01{}", "00".repeat(31))));
    message.signature = forged.clone();
    let tbs = group_message_tbs(
        &identity,
        &message.data,
        &message.sender_hmac,
        message.should_push,
    )?;
    let forged = Signature::from_slice(&forged)?;
    assert!(
        VerifyingKey::from_bytes(&identity)?
            .verify(&tbs, &forged)
            .is_ok(),
        "plain verify accepts the forgery"
    );
    assert_invalid_signature(envelope, "a forgery under the identity point");

    // (ii) A valid signature with S replaced by S + L.
    let keys = group_keys(1);
    let mut envelope = keyed_group_message_envelope(&keys, GroupMessageKind::Application, []);
    let message = group_message(&mut envelope);
    let mut signature = message.signature[..32].to_vec();
    signature.extend(plus_group_order(&message.signature[32..]));
    message.signature = signature;
    assert_invalid_signature(envelope, "S + L");

    // (iii) y = p + 3: it decodes to a point that is not of small order, and
    // `VerifyingKey::from_bytes(..).to_bytes()` returns it unchanged, so a
    // round-trip check would pass it.
    let non_canonical: [u8; 32] = bytes(&format!("f0{}7f", "ff".repeat(30)));
    let key = VerifyingKey::from_bytes(&non_canonical)?;
    assert_eq!(key.to_bytes(), non_canonical);
    assert!(!key.is_weak());
    let mut envelope = group_message_envelope(non_canonical, GroupMessageKind::Application, []);
    group_message(&mut envelope).signature = vec![0x55; 64];
    let parsed = parse_envelope(envelope)?;
    let error = verify_publish(&parsed).expect_err("a non-canonical key encoding is refused");
    assert!(
        matches!(
            error,
            ValidationError::PublishSignature(PublishSignatureError::NonCanonicalKey)
        ),
        "{error:?}"
    );
    assert_eq!(error.reason(), Reason::InvalidSignature);
}

/// A key that is canonical and not of small order, but not in the prime-order
/// subgroup, under which `verify_strict` accepts this signature with k reduced
/// modulo L, as the shared rule requires. Admission and the commit-log reader share one verifier,
/// so they agree on it.
#[xmtp_common::test(unwrap_try = true)]
// verifies: API-300
fn a_mixed_order_key_signature_that_api_300_accepts_is_admitted() {
    let commit_log_id: [u8; 32] = bytes(&format!("95{}", "99".repeat(31)));
    let mut signature = bytes::<32>(BASE_POINT).to_vec();
    signature.extend(bytes::<32>(
        "ffffd21b3430898396da71f15e68ebb28f557a6d362bf73dd7e50369cf67a40f",
    ));
    let entry = SealedCommitLogEntry {
        commit_log_id: commit_log_id.to_vec(),
        nonce: bytes::<12>(&format!("06{}", "00".repeat(11))).to_vec(),
        ciphertext: vec![0; 16],
        signature,
    };
    let tbs = commit_log_entry_tbs(
        &commit_log_id,
        entry.nonce.as_slice().try_into()?,
        &entry.ciphertext,
    )?;
    verify_publish_signature(&commit_log_id, &tbs, &entry.signature)?;
    let parsed = admit(ClientEnvelope {
        payload: Some(Payload::SealedCommitLogEntry(entry)),
    })?;
    assert_eq!(
        parsed.topic.cloned_vec(),
        expected_topic(TopicKind::CommitLogEntriesV1, commit_log_id)
    );
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: API-230, TOPIC-002
fn sealed_entry_fields_need_their_lengths() {
    let keys = group_keys(1);
    type Change = fn(&mut SealedCommitLogEntry, usize);
    let cases: [(SealedField, Change, &[usize]); 4] = [
        (
            SealedField::CommitLogId,
            |entry, length| entry.commit_log_id.resize(length, 0x55),
            &[0, 16, 31, 33],
        ),
        (
            SealedField::Nonce,
            |entry, length| entry.nonce.resize(length, 0x55),
            &[0, 11, 13],
        ),
        (
            SealedField::Ciphertext,
            |entry, length| entry.ciphertext.truncate(length),
            &[0, 15],
        ),
        (
            SealedField::Signature,
            |entry, length| entry.signature.resize(length, 0x55),
            &[0, 63, 65],
        ),
    ];
    for (field, change, lengths) in cases {
        for &length in lengths {
            let mut envelope = sealed_commit_log_envelope(&keys, 7);
            change(sealed_entry(&mut envelope), length);
            let expected = match field {
                SealedField::CommitLogId => |error: &ValidationError| {
                    matches!(
                        error,
                        ValidationError::SealedEntry(SealedField::CommitLogId)
                    )
                },
                SealedField::Nonce => |error: &ValidationError| {
                    matches!(error, ValidationError::SealedEntry(SealedField::Nonce))
                },
                SealedField::Ciphertext => |error: &ValidationError| {
                    matches!(error, ValidationError::SealedEntry(SealedField::Ciphertext))
                },
                SealedField::Signature => |error: &ValidationError| {
                    matches!(error, ValidationError::SealedEntry(SealedField::Signature))
                },
            };
            assert_malformed(envelope, expected, &format!("{field} of {length} bytes"));
        }
    }
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: API-230, API-300
fn any_change_to_a_sealed_entry_is_refused() {
    let keys = group_keys(1);
    let sealed = sealed_commit_log_envelope(&keys, 7);
    admit(sealed.clone())?;
    let other = group_keys(2).commit_log_id().to_vec();
    type Change = Box<dyn Fn(&mut SealedCommitLogEntry)>;
    let cases: [(&str, Change); 6] = [
        ("changed nonce", Box::new(|entry| entry.nonce[0] ^= 1)),
        (
            "flipped ciphertext byte",
            Box::new(|entry| entry.ciphertext[0] ^= 1),
        ),
        (
            "one appended ciphertext byte",
            Box::new(|entry| entry.ciphertext.push(0)),
        ),
        (
            "another group's commit-log id",
            Box::new(move |entry| entry.commit_log_id = other.clone()),
        ),
        (
            "flipped bit of S",
            Box::new(|entry| entry.signature[40] ^= 1),
        ),
        (
            "the group's message signature",
            Box::new(|entry| {
                let mut message =
                    keyed_group_message_envelope(&group_keys(1), GroupMessageKind::Commit, []);
                entry.signature = group_message(&mut message).signature.clone();
            }),
        ),
    ];
    for (case, change) in cases {
        let mut envelope = sealed.clone();
        change(sealed_entry(&mut envelope));
        assert_invalid_signature(envelope, case);
    }

    // The group signing key over the entry's own `CommitLogEntryTBS`.
    let mut envelope = sealed;
    let entry = sealed_entry(&mut envelope);
    let tbs = commit_log_entry_tbs(
        &keys.commit_log_id(),
        entry.nonce.as_slice().try_into()?,
        &entry.ciphertext,
    )?;
    entry.signature = raw_group_signing_key(1).sign(&tbs).to_bytes().to_vec();
    assert_invalid_signature(envelope, "the group signing key");
}
