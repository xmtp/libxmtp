//! Publish tests: attempt storage.

use super::*;

/// An unsettled batch is stored by its state's name. A renamed state fails
/// here instead of every stored row written after an `OUT_OF_RANGE` publish.
#[xmtp_common::test(unwrap_try = true)]
fn unsettled_states_are_stored_by_their_names() {
    let states = [
        (Unsettled::Attempt, "Attempt"),
        (Unsettled::Welcomes, "Welcomes"),
    ];
    assert_eq!(serde_variants::<Unsettled>(), states.map(|(_, name)| name));
    for (state, name) in states {
        let text = [&[0x60 | name.len() as u8], name.as_bytes()].concat();
        assert_eq!(xmtp_db::db_serialize(&state)?, text, "{name}");
        assert_eq!(xmtp_db::db_deserialize::<Unsettled>(&text)?, state);
    }
}

/// Each frozen attempt is unchanged.
#[xmtp_common::test(unwrap_try = true)]
fn golden_prepared_attempts_are_frozen() {
    for golden in GOLDEN_PREPARED_ATTEMPTS {
        assert_eq!(
            hex::encode(sha256(golden.text.as_bytes())),
            golden.sha256,
            "{} changed; frozen attempts are never edited",
            golden.name
        );
    }
}

/// The golden attempt's values that are not copied from a prepared message:
/// the base's proposal references, the receipts, the Welcome batch and the
/// standalone proposal.
fn golden_attempt_parts() -> (
    Vec<Vec<u8>>,
    Vec<Vec<u8>>,
    welcomes::PreparedWelcomes,
    prepared::PreparedProposal,
) {
    let receipt = xmtp_proto::backend_v1::EnvelopeMeta::default().encode_to_vec();
    (
        vec![vec![7, 8]],
        vec![receipt.clone()],
        welcomes::PreparedWelcomes {
            commit_sequence_id: 7,
            envelopes: vec![vec![1, 2, 3]],
            receipts: Some(vec![receipt]),
        },
        prepared::PreparedProposal {
            payload_hash: vec![9; 32],
            proposal: vec![4, 5, 6],
        },
    )
}

#[cfg(not(target_arch = "wasm32"))]
#[ignore = "writes fixtures/prepared_attempt_next.cbor.hex for a deliberate format change"]
#[xmtp_common::test(unwrap_try = true)]
async fn write_golden_prepared_attempt() {
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    group.send_message_optimistic(b"golden prepared attempt", Default::default())?;
    let (_, mut attempt) = prepare_message(&group).await?;
    let (base_proposals, receipts, welcomes, proposal) = golden_attempt_parts();
    attempt.base.proposals = base_proposals;
    attempt.receipts = Some(receipts);
    attempt.welcomes = Some(welcomes);
    attempt.proposals = vec![proposal];
    attempt.rejection = Some(rejection::PreparedRejection::sample());
    attempt.unsettled = Some(Unsettled::Welcomes);
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/groups/mls_sync/publish/fixtures/prepared_attempt_next.cbor.hex"),
        hex::encode(xmtp_db::db_serialize(&attempt)?) + "\n",
    )?;
}

/// Every frozen attempt decodes with every field in place: a renamed field,
/// nested or not, or a renamed variant fails here, as a required one fails to
/// decode and an optional one would read as `None`. The newest also encodes
/// back to its bytes; an older one may lack a field added later with a
/// default, which this build writes.
#[xmtp_common::test(unwrap_try = true)]
fn golden_prepared_attempt_keeps_every_field() {
    for (index, golden) in GOLDEN_PREPARED_ATTEMPTS.iter().enumerate() {
        let bytes = hex::decode(golden.text.trim())?;
        let attempt = PreparedAttempt::decode(&bytes)?;
        let (base_proposals, receipts, welcomes, proposal) = golden_attempt_parts();
        assert_eq!(attempt.version, prepared::PREPARED_ATTEMPT_VERSION);
        assert!(!attempt.base.authenticator.is_empty());
        assert_eq!(attempt.base.proposals, base_proposals);
        assert_eq!(attempt.payload_hash.len(), 32);
        assert_eq!(attempt.envelopes.len(), 1);
        assert_eq!(attempt.proposals, [proposal]);
        assert_eq!(attempt.receipts, Some(receipts));
        assert_eq!(attempt.welcomes, Some(welcomes));
        assert_eq!(
            attempt.rejection,
            Some(rejection::PreparedRejection::sample())
        );
        assert_eq!(attempt.unsettled, Some(Unsettled::Welcomes));
        if index == GOLDEN_PREPARED_ATTEMPTS.len() - 1 {
            assert_eq!(xmtp_db::db_serialize(&attempt)?, bytes, "{}", golden.name);
        }
    }
}

/// Every byte field of the newest frozen attempt, and of the same attempt as
/// this build writes it, is a CBOR byte string, not an array of integers: the
/// base's authenticator and proposal references, the payload hash, the
/// envelopes, the standalone proposal's hash and bytes, the receipts, and the
/// Welcome batch's envelopes and receipts. No stored array holds an integer.
#[xmtp_common::test(unwrap_try = true)]
fn stored_attempt_byte_fields_are_byte_strings() {
    use cbor4ii::core::{Value, dec::Decode as _, utils::SliceReader};

    fn walk(value: &Value, byte_strings: &mut usize) {
        match value {
            Value::Bytes(_) => *byte_strings += 1,
            Value::Array(items) => {
                for item in items {
                    assert!(
                        !matches!(item, Value::Integer(_)),
                        "an array of integers: {value:?}"
                    );
                    walk(item, byte_strings);
                }
            }
            Value::Map(entries) => entries
                .iter()
                .for_each(|(_, value)| walk(value, byte_strings)),
            Value::Tag(_, inner) => walk(inner, byte_strings),
            _ => {}
        }
    }
    let golden = GOLDEN_PREPARED_ATTEMPTS.last().unwrap();
    let frozen = hex::decode(golden.text.trim())?;
    let written = xmtp_db::db_serialize(&PreparedAttempt::decode(&frozen)?)?;
    for (what, bytes) in [(golden.name, &frozen), ("this build", &written)] {
        let mut byte_strings = 0;
        walk(
            &Value::decode(&mut SliceReader::new(bytes))?,
            &mut byte_strings,
        );
        assert_eq!(byte_strings, 9, "{what}");
    }
}

/// The stored attempt is CBOR: a map keyed by field names, so its first byte
/// is the map header, not the version. Decoding still rejects another
/// version, a row in another format, and a cut row. A row written before a
/// later `Option` field existed reads it as `None`.
#[xmtp_common::test(unwrap_try = true)]
async fn prepared_attempt_is_cbor_and_checks_its_version() {
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    group.send_message_optimistic(b"cbor prepared format", Default::default())?;
    let (_, attempt) = prepare_message(&group).await?;

    let invalid = |bytes: &[u8]| {
        matches!(
            PreparedAttempt::decode(bytes),
            Err(GroupError::OutgoingPreparation(
                OutgoingPreparationError::InvalidPreparedAttempt
            ))
        )
    };

    let bytes = xmtp_db::db_serialize(&attempt)?;
    // A map of nine fields.
    assert_eq!(bytes[0], 0xa9);
    assert_eq!(PreparedAttempt::decode(&bytes)?, attempt);

    let other_version = PreparedAttempt {
        version: prepared::PREPARED_ATTEMPT_VERSION + 1,
        ..attempt.clone()
    };
    assert!(invalid(&xmtp_db::db_serialize(&other_version)?));
    // A row that starts with the version byte, as a positional format would.
    assert!(invalid(
        &[&[prepared::PREPARED_ATTEMPT_VERSION][..], &bytes[1..]].concat()
    ));
    assert!(invalid(&bytes[..bytes.len() - 1]));

    // The same row without its last field, `unsettled`: a map of eight
    // fields, as a build before that field would have written it.
    #[derive(serde::Serialize)]
    struct WithoutUnsettled<'a> {
        version: u8,
        base: &'a prepared::PreparedBase,
        #[serde(with = "xmtp_db::byte_string")]
        payload_hash: &'a [u8],
        #[serde(with = "xmtp_db::byte_strings")]
        envelopes: &'a [Vec<u8>],
        proposals: &'a [prepared::PreparedProposal],
        #[serde(with = "xmtp_db::optional_byte_strings")]
        receipts: &'a Option<Vec<Vec<u8>>>,
        welcomes: &'a Option<welcomes::PreparedWelcomes>,
        rejection: &'a Option<rejection::PreparedRejection>,
    }
    let older = xmtp_db::db_serialize(&WithoutUnsettled {
        version: attempt.version,
        base: &attempt.base,
        payload_hash: &attempt.payload_hash,
        envelopes: &attempt.envelopes,
        proposals: &attempt.proposals,
        receipts: &attempt.receipts,
        welcomes: &attempt.welcomes,
        rejection: &attempt.rejection,
    })?;
    assert_eq!(older[0], 0xa8);
    let restored = PreparedAttempt::decode(&older)?;
    assert_eq!(restored.unsettled, None);
    assert!(restored.same_attempt(&attempt));
}
