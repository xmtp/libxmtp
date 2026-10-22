use super::*;
use crate::tester;
use prost::Message;
use xmtp_mls_validation::commit::CommitRuleError;
use xmtp_proto::types::Topic;

mod deadlines;
mod dictionary_creation;
mod faults;
mod membership_component;
mod membership_recovery;
mod out_of_range_settlement;
mod send_state_transitions;
mod signing;

mod transcript;

// verifies: GMOD-036
#[rstest::rstest]
#[case::name_first(true)]
#[case::description_first(false)]
#[xmtp_common::test(unwrap_try = true)]
async fn competing_metadata_attempts_from_one_epoch_preserve_both_updates(
    #[case] name_first: bool,
) -> Result<(), GroupError> {
    use crate::groups::send_message_opts::SendMessageOpts;
    use xmtp_db::group_message::MsgQueryArgs;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    let bo_group = xmtp_common::wait_for_ok(|| async {
        bo.sync_welcomes()
            .await
            .and_then(|_| bo.group(&group.group_id).map_err(GroupError::from))
    })
    .await
    .expect("the published Welcome must become visible");
    let caro_group = xmtp_common::wait_for_ok(|| async {
        caro.sync_welcomes()
            .await
            .and_then(|_| caro.group(&group.group_id).map_err(GroupError::from))
    })
    .await
    .expect("the published Welcome must become visible");
    bo_group.receive().await?;
    caro_group.receive().await?;
    let name = QueueIntent::metadata_update()
        .data(Vec::<u8>::from(
            UpdateMetadataIntentData::new_update_group_name("competing name".into()),
        ))
        .queue(&group)?;
    let description = QueueIntent::metadata_update()
        .data(Vec::<u8>::from(
            UpdateMetadataIntentData::new_update_group_description("competing description".into()),
        ))
        .queue(&bo_group)?;

    // Hold both durable attempts before either can reach backend order.
    let (_, first) = prepare_kind(&group, IntentKind::MetadataUpdate).await?;
    let (_, second) = prepare_kind(&bo_group, IntentKind::MetadataUpdate).await?;
    assert_eq!(first.base, second.base);
    assert_eq!(group.epoch().await?, first.base.epoch);
    assert_eq!(bo_group.epoch().await?, first.base.epoch);
    assert_ne!(first.envelopes, second.envelopes);
    if name_first {
        group.publish_intents().await?;
        bo_group.publish_intents().await?;
    } else {
        bo_group.publish_intents().await?;
        group.publish_intents().await?;
    }
    group.sync_until_intent_resolved(name.id).await?;
    bo_group.sync_until_intent_resolved(description.id).await?;
    let topic = Topic::new_group_message(group.group_id);
    let key = xmtp_db::incoming_envelope::StreamTopic::group(group.group_id);
    let target = group
        .context
        .db()
        .topic_progress(&key)?
        .processed
        .max(bo_group.context.db().topic_progress(&key)?.processed);
    for peer in [&group, &bo_group, &caro_group] {
        crate::subscriptions::barrier::wait_through(
            &peer.context,
            [(topic.clone(), target)].into(),
            None,
        )
        .await
        .expect("all members must process both completed metadata changes");
        assert_eq!(peer.group_name()?, "competing name");
        assert_eq!(peer.group_description()?, "competing description");
        assert_eq!(peer.epoch().await?, first.base.epoch + 2);
        assert_eq!(
            peer.epoch_authenticator().await?,
            group.epoch_authenticator().await?
        );
    }
    for (sender, body) in [
        (&group, b"from alix".as_slice()),
        (&bo_group, b"from bo"),
        (&caro_group, b"from caro"),
    ] {
        sender
            .send_message(body, SendMessageOpts::default())
            .await?;
    }
    let target = caro_group.context.db().topic_progress(&key)?.processed;
    for peer in [&group, &bo_group, &caro_group] {
        crate::subscriptions::barrier::wait_through(
            &peer.context,
            [(topic.clone(), target)].into(),
            None,
        )
        .await
        .expect("all members must process the last confirmed send");
        let messages = peer.find_messages(&MsgQueryArgs::default())?;
        for body in [b"from alix".as_slice(), b"from bo", b"from caro"] {
            assert_eq!(
                messages
                    .iter()
                    .filter(|message| message.decrypted_message_bytes == body)
                    .count(),
                1
            );
        }
    }
    Ok(())
}

async fn prepare_message<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
) -> Result<(StoredGroupIntent, PreparedAttempt), GroupError> {
    prepare_kind(group, IntentKind::SendMessage).await
}

async fn prepare_kind<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    kind: IntentKind,
) -> Result<(StoredGroupIntent, PreparedAttempt), GroupError> {
    let requirements = crate::state_tx::state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls_group, storage| {
            let intent = storage
                .db()
                .find_group_intents(
                    group.group_id,
                    Some(vec![IntentState::ToPublish]),
                    Some(vec![kind]),
                )?
                .into_iter()
                .next()
                .ok_or(GroupError::UninitializedResult)?;
            PublishRequirements::capture(mls_group, &intent).map(Continue)
        })
    })?
    .into_continued();
    let mut dependencies = group.resolve_publish_dependencies(&requirements).await?;
    let attempt = group
        .prepare_publish_attempt(&requirements, &mut dependencies)?
        .ok_or(GroupError::UninitializedResult)?;
    Ok((requirements.intent, attempt))
}

/// A frozen stored attempt: its file name, its hex text, and the SHA-256 of
/// the text.
struct GoldenAttempt {
    name: &'static str,
    text: &'static str,
    sha256: &'static str,
}

/// Stored attempts with every optional field set: receipts, a Welcome batch
/// with receipts, a rejection, an unsettled batch, a standalone proposal, and
/// a proposal reference in the base. Every name in them, nested ones included, is stored in rows of
/// in-flight intents, so each file is frozen; the newest is the format this
/// build writes. For a deliberate format change, run
/// `dev/nix-shell 'just test workspace -p xmtp_mls write_golden_prepared_attempt --run-ignored only'`,
/// rename `fixtures/prepared_attempt_next.cbor.hex` to
/// `prepared_attempt_<release>.cbor.hex`, and add it last here with its hash.
const GOLDEN_PREPARED_ATTEMPTS: &[GoldenAttempt] = &[GoldenAttempt {
    name: "prepared_attempt_rc4.cbor.hex",
    text: include_str!("fixtures/prepared_attempt_rc4.cbor.hex"),
    sha256: "b4581c9dd5e748865c08f0db462f85d4165b8310a1e9e7ee6d8cbab5ca99d722",
}];

/// The names serde stores the variants of the enum `T` by, in declaration
/// order, read from its derived `Deserialize`. A renamed, added or removed
/// variant changes the list.
pub(super) fn serde_variants<T: serde::de::DeserializeOwned>() -> &'static [&'static str] {
    use serde::de::{Error, Visitor};

    #[derive(Debug)]
    struct Stop;
    impl std::fmt::Display for Stop {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("not an enum")
        }
    }
    impl std::error::Error for Stop {}
    impl Error for Stop {
        fn custom<M: std::fmt::Display>(_: M) -> Self {
            Stop
        }
    }
    struct Variants(Option<&'static [&'static str]>);
    impl<'de> serde::Deserializer<'de> for &mut Variants {
        type Error = Stop;
        fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Stop> {
            Err(Stop)
        }
        fn deserialize_enum<V: Visitor<'de>>(
            self,
            _: &'static str,
            variants: &'static [&'static str],
            _: V,
        ) -> Result<V::Value, Stop> {
            self.0 = Some(variants);
            Err(Stop)
        }
        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes
            byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct map
            struct identifier ignored_any
        }
    }
    let mut variants = Variants(None);
    let _ = T::deserialize(&mut variants);
    variants.0.expect("an enum")
}

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
    tester!(alix, disable_workers);
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
    tester!(alix, disable_workers);
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

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn rejected_intent_keeps_its_typed_cause_after_restart_and_later_rejection() {
    use crate::{
        Client,
        builder::DeviceSyncMode,
        groups::intents::{PermissionPolicyOption, PermissionUpdateType},
        identity::IdentityStrategy,
        utils::DefaultTestClientCreator,
    };
    use std::sync::Arc;
    use xmtp_common::wait_for_some;
    use xmtp_db::{
        StorageOption, TestDb, XmtpTestDb,
        incoming_envelope::{QueryIncomingEnvelope, StreamTopic},
    };
    use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;
    use xmtp_mls_common::group_mutable_metadata::MetadataField;
    use xmtp_proto::{api_client::ApiBuilder, prelude::XmtpTestClient};

    tester!(alix, persistent_db, disable_workers);
    tester!(bo, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;

    group
        .update_permission_policy(
            PermissionUpdateType::UpdateMetadata,
            PermissionPolicyOption::Deny,
            Some(MetadataField::GroupName),
        )
        .await?;
    let original_name = group.group_name()?;
    let original_authenticator = group.epoch_authenticator().await?;
    let result = group.update_group_name("denied update".into()).await;
    let Err(GroupError::Sync(summary)) = result else {
        panic!("expected a typed intent rejection, got {result:?}");
    };
    assert!(summary.is_errored());
    let rejected_cursor = summary
        .process
        .errored
        .iter()
        .find_map(|(cursor, error)| {
            matches!(
                error,
                GroupMessageProcessingError::CommitValidation(CommitValidationError::Rule(
                    CommitRuleError::InsufficientPermissions
                ))
            )
            .then_some(*cursor)
        })
        .unwrap();
    let failed = group
        .context
        .db()
        .find_group_intents(
            group.group_id,
            Some(vec![IntentState::Error]),
            Some(vec![IntentKind::MetadataUpdate]),
        )?
        .pop()
        .unwrap();
    let saved = group.context.db().prepared_envelopes(failed.id)?.unwrap();
    let prepared = PreparedAttempt::decode(&saved)?;
    assert!(prepared.rejection.is_some());

    // A later real backend envelope replaces only the topic diagnostic.
    let mut malformed = ClientEnvelope::decode(prepared.envelopes.last().unwrap().as_slice())?;
    let Some(Payload::GroupMessage(message)) = &mut malformed.payload else {
        panic!("expected a group envelope");
    };
    // Keep valid TLS framing so backend admission reaches the client rejection path.
    *message.data.last_mut().unwrap() ^= 1;
    let receipts = group
        .context
        .api()
        .publish_units(vec![PublishUnit::single(malformed)?])
        .await?;
    let topic = Topic::new_group_message(group.group_id);
    let (_, later_cursor, _) = xmtp_api_backend::envelope::metadata(&receipts[0], topic.kind())?;
    assert!(later_cursor > rejected_cursor);
    group.receive().await?;
    let stream_topic = StreamTopic::group(group.group_id);
    let later_rejection = group
        .context
        .db()
        .read_last_rejection(&stream_topic)?
        .unwrap();
    assert_eq!(later_rejection.sequence_id, later_cursor);
    assert_eq!(later_rejection.code, "own_message_without_attempt");
    assert_eq!(group.group_name()?, original_name);
    assert_eq!(group.epoch_authenticator().await?, original_authenticator);

    let database = match alix.context.store().opts() {
        StorageOption::Persistent(path) => path.clone(),
        StorageOption::Ephemeral => panic!("restart test needs a persistent database"),
    };
    let group_id = group.group_id;
    assert!(
        wait_for_some(|| async {
            alix.context
                .incoming_runtime()
                .coordinator
                .lock()
                .is_none()
                .then_some(())
        })
        .await
        .is_some()
    );
    drop(group);
    drop(alix);

    // Reopen the original file and cached identity after dropping the first client.
    let store = TestDb::create_persistent_store(Some(database)).await;
    let api = Arc::new(DefaultTestClientCreator::create().build()?);
    let restarted = Client::builder(IdentityStrategy::CachedOnly)
        .store(store)
        .api_client(api)
        .default_mls_store()?
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .with_disable_workers(true)
        .with_commit_log_worker(false)
        .with_device_sync_worker_mode(Some(DeviceSyncMode::Disabled))
        .build()
        .await?;
    let group = restarted.group(&group_id)?;
    let result = group.sync_until_intent_resolved(failed.id).await;
    let Err(GroupError::Sync(summary)) = result else {
        panic!("reopened intent lost its rejection: {result:?}");
    };
    assert!(summary.is_errored());
    assert!(summary.process.errored.iter().any(|(cursor, error)| {
        *cursor == rejected_cursor
            && matches!(
                error,
                GroupMessageProcessingError::CommitValidation(CommitValidationError::Rule(
                    CommitRuleError::InsufficientPermissions
                ))
            )
    }));
    assert_eq!(
        group.context.db().prepared_envelopes(failed.id)?.unwrap(),
        saved
    );
    assert_eq!(
        group
            .context
            .db()
            .read_last_rejection(&stream_topic)?
            .unwrap(),
        later_rejection
    );
    assert_eq!(group.group_name()?, original_name);
    assert_eq!(group.epoch_authenticator().await?, original_authenticator);
}

#[xmtp_common::test(unwrap_try = true)]
async fn prepared_proposals_keep_wire_order_after_reload() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix.create_group(None, None)?;
    let data: Vec<u8> =
        ProposeMemberUpdateIntentData::new(vec![bo.inbox_id().to_string()], vec![]).try_into()?;
    QueueIntent::propose_member_update()
        .data(data)
        .queue(&group)?;
    let (_, attempt) = prepare_kind(&group, IntentKind::ProposeMemberUpdate).await?;
    assert_eq!(attempt.proposals.len(), 2);
    for (index, bytes) in attempt.envelopes.iter().enumerate() {
        let envelope = ClientEnvelope::decode(bytes.as_slice())?;
        let Some(Payload::GroupMessage(message)) = envelope.payload else {
            panic!("expected group message");
        };
        let proposal = attempt
            .proposal_for_payload(&sha256(&message.data))?
            .unwrap();
        match index {
            0 => assert!(matches!(proposal.proposal(), Proposal::Add(_))),
            1 => assert!(matches!(proposal.proposal(), Proposal::AppDataUpdate(_))),
            _ => panic!("unexpected proposal"),
        }
    }
    let pending = crate::state_tx::state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls_group, _| {
            Ok::<_, GroupError>(Continue(mls_group.pending_proposals().count()))
        })
    })?
    .into_continued();
    assert_eq!(pending, 0, "unreceived proposals entered committed state");
}

#[xmtp_common::test(unwrap_try = true)]
async fn lost_publish_reply_retries_exact_prepared_envelopes() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    group.send_message_optimistic(b"lost publish reply", Default::default())?;
    let (intent, attempt) = prepare_message(&group).await?;
    assert_eq!(group.published_intent_target(intent.id)?, None);

    // The backend accepted the request, but the client did not save the reply.
    group
        .context
        .api()
        .send_group_messages(vec![attempt.publish_unit(group.context.api().limits())?])
        .await?;
    let group_id = group.group_id;
    let snapshot = std::sync::Arc::new(alix.db_snapshot());
    drop(group);
    drop(alix);
    tester!(restarted, snapshot: snapshot, disable_workers);
    let group = restarted.group(&group_id)?;
    let before = group.context.db().prepared_envelopes(intent.id)?.unwrap();
    group.publish_intents().await?;
    let published_target = group.published_intent_target(intent.id)?.unwrap();

    let after =
        PreparedAttempt::decode(&group.context.db().prepared_envelopes(intent.id)?.unwrap())?;
    assert_eq!(after.envelopes, PreparedAttempt::decode(&before)?.envelopes);
    assert!(after.receipts.is_some());
    let rows = group
        .context
        .api()
        .query_all(
            [(Topic::new_group_message(group.group_id), Cursor(0))].into(),
            group.context.api().limits().max_query_limit as u32,
        )
        .await?;
    let occurrences = rows
        .iter()
        .filter(|row| {
            row.envelope
                .as_ref()
                .is_some_and(|envelope| envelope.encode_to_vec() == attempt.envelopes[0])
        })
        .count();
    assert_eq!(occurrences, 1, "retry created another backend envelope");
    let accepted = rows
        .iter()
        .find(|row| {
            row.envelope
                .as_ref()
                .is_some_and(|envelope| envelope.encode_to_vec() == attempt.envelopes[0])
        })
        .unwrap();
    assert_eq!(
        published_target.0,
        accepted
            .meta
            .as_ref()
            .unwrap()
            .cursor
            .as_ref()
            .unwrap()
            .sequence_id
    );
}

// verifies: SEND-008
#[xmtp_common::test(unwrap_try = true)]
async fn late_publish_reply_cannot_update_replacement_attempt() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let message_id = group.send_message_optimistic(b"late publish reply", Default::default())?;
    let (old_intent, old_attempt) = prepare_message(&group).await?;
    let receipts = group
        .context
        .api()
        .send_group_messages(vec![
            old_attempt.publish_unit(group.context.api().limits())?,
        ])
        .await?;

    // Model ordered supersession while the first publish reply is delayed.
    crate::state_tx::state_write(group.context.mls_storage(), |tx| {
        tx.storage()
            .db()
            .set_group_intent_to_publish(old_intent.id)?;
        Ok::<_, GroupError>(Continue(()))
    })?;
    let (_, replacement) = prepare_message(&group).await?;
    assert_ne!(replacement.envelopes, old_attempt.envelopes);
    group.record_publish_receipts(&old_intent, &old_attempt, receipts)?;

    let current = PreparedAttempt::decode(
        &group
            .context
            .db()
            .prepared_envelopes(old_intent.id)?
            .unwrap(),
    )?;
    assert_eq!(current, replacement);
    let message: StoredGroupMessage = group.context.db().fetch(&message_id)?.unwrap();
    assert!(message.envelope_hash.is_none());
    assert!(message.expiry_ns.is_none());
}

#[xmtp_common::test(unwrap_try = true)]
async fn oversized_unprepared_message_does_not_block_later_intents() {
    use xmtp_db::group_message::DeliveryStatus;

    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let status_events = alix.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::MessageStatusChanged]),
        Some(10),
    );
    let oversized = vec![42; xmtp_configuration::BACKEND_DEFAULT_MAX_ENVELOPE_BYTES + 1];
    let failed_id = group.send_message_optimistic(&oversized, Default::default())?;
    let later_id = group.send_message_optimistic(b"after rejected request", Default::default())?;
    assert!(matches!(
        group.publish_intents().await,
        Err(GroupError::WrappedApi(xmtp_api::ApiError::EnvelopeTooLarge))
    ));

    let failed: StoredGroupMessage = group.context.db().fetch(&failed_id)?.unwrap();
    assert_eq!(failed.delivery_status, DeliveryStatus::Failed);
    assert!(matches!(
        status_events.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::MessageStatusChanged(change)), ..
        }] if change.message_id == failed_id
            && change.previous == xmtp_events::MessageStatus::Unpublished
            && change.current == xmtp_events::MessageStatus::Failed
    ));
    let later: StoredGroupMessage = group.context.db().fetch(&later_id)?.unwrap();
    assert!(later.envelope_hash.is_some());
    let rejected = group.context.db().find_group_intents(
        group.group_id,
        Some(vec![IntentState::Error]),
        Some(vec![IntentKind::SendMessage]),
    )?;
    assert_eq!(rejected.len(), 1);
    assert!(
        group
            .context
            .db()
            .prepared_envelopes(rejected[0].id)?
            .is_none()
    );
}

// verifies: SEND-009
#[xmtp_common::test(unwrap_try = true)]
async fn malformed_admin_list_intent_does_not_block_later_intents() {
    use crate::groups::intents::{AdminListActionType, UpdateAdminListIntentData};

    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    // Earlier builds queued the caller's inbox id without parsing it.
    let data: Vec<u8> = UpdateAdminListIntentData::new(AdminListActionType::Add, "x".into()).into();
    let malformed = QueueIntent::update_admin_list().data(data).queue(&group)?;

    group
        .update_group_name("after malformed intent".into())
        .await?;
    assert_eq!(group.group_name()?, "after malformed intent");
    let failed = Fetch::<StoredGroupIntent>::fetch(&group.context.db(), &malformed.id)??;
    assert_eq!(failed.state, IntentState::Error);
    assert!(group.context.db().prepared_envelopes(failed.id)?.is_none());
    assert!(group.admin_list()?.is_empty());
}

#[xmtp_common::test(unwrap_try = true)]
async fn preparation_rejects_changed_proposal_refs_at_the_same_epoch() -> Result<(), GroupError> {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix.create_group(None, None)?;
    let proposal = QueueIntent::propose_member_update()
        .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
            vec![bo.inbox_id().to_string()],
            vec![],
        ))?)
        .queue(&group)?;
    group.sync_until_intent_resolved(proposal.id).await?;
    let intent = QueueIntent::commit_pending_proposals().queue(&group)?;
    let requirements = crate::state_tx::state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls, storage| {
            let intent = Fetch::<StoredGroupIntent>::fetch(&storage.db(), &intent.id)?
                .ok_or(GroupError::UninitializedResult)?;
            PublishRequirements::capture(mls, &intent).map(Continue)
        })
    })?
    .into_continued();
    let mut dependencies = group.resolve_publish_dependencies(&requirements).await?;
    let original = group.with_group_snapshot(PreparedBase::capture)?;
    let reference = group.with_group_snapshot(|mls| {
        let pending = mls.pending_proposals().collect::<Vec<_>>();
        assert!(pending.len() >= 2);
        Ok(pending[0].proposal_reference_ref().clone())
    })?;
    crate::state_tx::state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls, storage| {
            mls.remove_pending_proposal(storage, &reference).unwrap();
            Ok::<_, GroupError>(Continue(()))
        })
    })?;
    let changed = group.with_group_snapshot(PreparedBase::capture)?;
    assert_eq!(changed.epoch, original.epoch);
    assert_eq!(changed.authenticator, original.authenticator);
    assert_ne!(changed, original);
    assert!(matches!(
        group.prepare_publish_attempt(&requirements, &mut dependencies),
        Err(GroupError::OutgoingPreparation(
            OutgoingPreparationError::StateChanged
        ))
    ));
    assert_eq!(group.with_group_snapshot(PreparedBase::capture)?, changed);
    assert!(group.context.db().prepared_envelopes(intent.id)?.is_none());
    Ok(())
}

#[xmtp_common::test(unwrap_try = true)]
async fn preparation_fences_a_second_snapshot_of_the_same_intent() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let data = group
        .get_membership_update_intent(&[bo.inbox_id()], &[])
        .await?;
    QueueIntent::update_group_membership()
        .data(data)
        .queue(&group)?;
    let requirements = crate::state_tx::state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls_group, storage| {
            let intent = storage
                .db()
                .find_group_intents(
                    group.group_id,
                    Some(vec![IntentState::ToPublish]),
                    Some(vec![IntentKind::UpdateGroupMembership]),
                )?
                .remove(0);
            PublishRequirements::capture(mls_group, &intent).map(Continue)
        })
    })?
    .into_continued();
    let mut first_dependencies = group.resolve_publish_dependencies(&requirements).await?;
    let mut second_dependencies = group.resolve_publish_dependencies(&requirements).await?;
    let first = group
        .prepare_publish_attempt(&requirements, &mut first_dependencies)?
        .unwrap();
    let second = group.prepare_publish_attempt(&requirements, &mut second_dependencies);
    assert!(matches!(
        second,
        Err(GroupError::OutgoingPreparation(
            OutgoingPreparationError::StateChanged
        ))
    ));
    let current = PreparedAttempt::decode(
        &group
            .context
            .db()
            .prepared_envelopes(requirements.intent.id)?
            .unwrap(),
    )?;
    assert_eq!(current, first);

    let later = QueueIntent::update_group_membership()
        .data(requirements.intent.data.as_slice().to_vec())
        .queue(&group)?;
    assert!(matches!(
        prepare_kind(&group, IntentKind::UpdateGroupMembership).await,
        Err(GroupError::OutgoingPreparation(
            OutgoingPreparationError::StateChanged
        ))
    ));
    assert!(group.context.db().prepared_envelopes(later.id)?.is_none());
    assert_eq!(
        group
            .context
            .db()
            .find_group_intents(
                group.group_id,
                Some(vec![IntentState::Published]),
                Some(vec![IntentKind::UpdateGroupMembership]),
            )?
            .len(),
        1
    );
}

// verifies: SEND-006, SEND-011
#[xmtp_common::test(unwrap_try = true)]
async fn welcome_followup_retries_exact_bytes_after_restart() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let data = group
        .get_membership_update_intent(&[bo.inbox_id()], &[])
        .await?;
    QueueIntent::update_group_membership()
        .data(data)
        .queue(&group)?;
    let (intent, _) = prepare_kind(&group, IntentKind::UpdateGroupMembership).await?;
    group.publish_intents().await?;
    assert!(!group.receive().await?.is_errored());
    let prepared = group.prepare_required_welcomes(intent.id)?.unwrap();
    let welcomes = prepared.welcomes.as_ref().unwrap();
    group
        .context
        .api()
        .publish_units(welcomes.units(group.context.api().limits())?)
        .await?;

    let group_id = group.group_id;
    let snapshot = std::sync::Arc::new(alix.db_snapshot());
    drop(group);
    drop(alix);
    tester!(restarted, snapshot: snapshot, disable_workers);
    let group = restarted.group(&group_id)?;
    let before: StoredGroupIntent = group.context.db().fetch(&intent.id)?.unwrap();
    assert_eq!(before.state, IntentState::Committed);
    let summary = group.sync_until_last_intent_resolved().await?;
    assert!(!summary.is_errored());

    let current: StoredGroupIntent = group.context.db().fetch(&intent.id)?.unwrap();
    assert_eq!(current.state, IntentState::Processed);
    let after =
        PreparedAttempt::decode(&group.context.db().prepared_envelopes(intent.id)?.unwrap())?;
    let after = after.welcomes.unwrap();
    assert_eq!(after.envelopes, welcomes.envelopes);
    assert_eq!(
        after.receipts.as_ref().unwrap().len(),
        after.envelopes.len()
    );
    let received = group
        .context
        .api()
        .query_welcome_messages(bo.installation_public_key())
        .await?;
    assert_eq!(received.len(), 1, "retry created a second Welcome");
}

#[xmtp_common::test(unwrap_try = true)]
async fn welcome_followup_requires_ordered_commit_cursor() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let data = group
        .get_membership_update_intent(&[bo.inbox_id()], &[])
        .await?;
    QueueIntent::update_group_membership()
        .data(data)
        .queue(&group)?;
    let (intent, _) = prepare_kind(&group, IntentKind::UpdateGroupMembership).await?;
    crate::state_tx::state_write(group.context.mls_storage(), |tx| {
        tx.storage()
            .db()
            .set_group_intent_committed(intent.id, Cursor(0))?;
        Ok::<_, GroupError>(Continue(()))
    })?;
    assert!(matches!(
        group.prepare_required_welcomes(intent.id),
        Err(GroupError::OutgoingPreparation(
            OutgoingPreparationError::InvalidPreparedAttempt
        ))
    ));
    let after =
        PreparedAttempt::decode(&group.context.db().prepared_envelopes(intent.id)?.unwrap())?;
    assert!(after.welcomes.is_none());
    let current: StoredGroupIntent = group.context.db().fetch(&intent.id)?.unwrap();
    assert_eq!(current.state, IntentState::Committed);
}

#[xmtp_common::test(unwrap_try = true)]
async fn intent_sync_rejects_an_absent_intent() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let absent_id = i32::MAX;
    assert!(Fetch::<StoredGroupIntent>::fetch(&group.context.db(), &absent_id)?.is_none());
    assert!(matches!(
        group.sync_until_intent_resolved(absent_id).await,
        Err(GroupError::NotFound(xmtp_db::NotFound::IntentById(id))) if id == absent_id
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn send_completes_after_queued_state_changes() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    bo.sync_welcomes().await?;
    let peer = bo.group(&group.group_id)?;
    let queued_changes = MAX_GROUP_SYNC_RETRIES + 1;
    for index in 0..queued_changes {
        QueueIntent::metadata_update()
            .data(Vec::<u8>::from(
                UpdateMetadataIntentData::new_update_group_name(format!("queued {index}")),
            ))
            .queue(&group)?;
    }

    group
        .send_message(b"after queued changes", Default::default())
        .await?;

    peer.sync().await?;
    let expected_name = format!("queued {}", queued_changes - 1);
    assert_eq!(group.group_name()?, expected_name);
    assert_eq!(peer.group_name()?, expected_name);
    assert_eq!(
        group.epoch_authenticator().await?,
        peer.epoch_authenticator().await?
    );
    let messages = peer.find_messages(&xmtp_db::group_message::MsgQueryArgs::default())?;
    assert_eq!(
        messages
            .iter()
            .filter(|message| message.decrypted_message_bytes == b"after queued changes")
            .count(),
        1
    );
}

// verifies: GMOD-018
#[xmtp_common::test(unwrap_try = true)]
async fn pending_adds_welcome_every_new_leaf_after_commit_readback() {
    use crate::groups::intents::{CommitPendingProposalsIntentData, ProposeMemberUpdateIntentData};
    use xmtp_db::incoming_envelope::{QueryIncomingEnvelope, StreamTopic};
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = xmtp_common::wait_for_ok(|| async {
        bo.sync_welcomes().await?;
        bo.group(&group.group_id).map_err(GroupError::from)
    })
    .await?;
    bo_group.receive().await?;
    tester!(bo2, from: bo, disable_workers);
    tester!(caro, disable_workers);
    tester!(caro2, from: caro, disable_workers);
    let proposal = QueueIntent::propose_member_update()
        .data(Vec::<u8>::try_from(ProposeMemberUpdateIntentData::new(
            vec![bo.inbox_id().into(), caro.inbox_id().into()],
            vec![],
        ))?)
        .queue(&group)?;
    group.sync_until_intent_resolved(proposal.id).await?;
    let pending = group.with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))?;
    assert!(pending >= 3);
    xmtp_common::wait_for_some(|| async {
        bo_group.receive().await.ok()?;
        (bo_group
            .with_group_snapshot(|mls| Ok(mls.pending_proposals().count()))
            .ok()?
            == pending)
            .then_some(())
    })
    .await?;
    let intent = QueueIntent::commit_pending_proposals()
        .data(Vec::<u8>::from(CommitPendingProposalsIntentData::default()))
        .queue(&bo_group)?;
    prepare_kind(&bo_group, IntentKind::CommitPendingProposals).await?;
    bo_group.publish_intents().await?;
    let published: StoredGroupIntent = bo.context.db().fetch(&intent.id)?.unwrap();
    assert_eq!(published.state, IntentState::Published);
    assert!(bo_group.prepare_required_welcomes(intent.id)?.is_none());
    for joiner in [&bo2, &caro, &caro2] {
        assert!(
            bo.context
                .api()
                .query_welcome_messages(joiner.installation_public_key())
                .await?
                .is_empty()
        );
    }
    bo_group.sync_until_intent_resolved(intent.id).await?;
    let committed: StoredGroupIntent = bo.context.db().fetch(&intent.id)?.unwrap();
    assert_eq!(committed.state, IntentState::Processed);
    let cursor = Cursor(committed.sequence_id.unwrap() as u64);
    assert!(cursor.0 > 0);
    assert!(
        bo.context
            .db()
            .topic_progress(&StreamTopic::group(group.group_id))?
            .processed
            >= cursor
    );
    for joiner in [&bo2, &caro, &caro2] {
        let joined = xmtp_common::wait_for_ok(|| async {
            joiner.sync_welcomes().await?;
            joiner.group(&group.group_id).map_err(GroupError::from)
        })
        .await?;
        assert_eq!(
            joiner
                .context
                .db()
                .topic_progress(&StreamTopic::group(group.group_id))?
                .processed,
            cursor
        );
        assert_eq!(
            joined.epoch_authenticator().await?,
            bo_group.epoch_authenticator().await?
        );
        assert_eq!(joined.members().await?.len(), 3);
        assert_eq!(
            joined.with_group_snapshot(|mls| Ok(mls.members().count()))?,
            5
        );
    }
}

// verifies: SEND-016
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn rejected_own_application_echo_fails_atomically_and_survives_restart() {
    use crate::{
        Client, builder::DeviceSyncMode, groups::mls_sync::GroupHeadOutcome,
        identity::IdentityStrategy, utils::DefaultTestClientCreator,
    };
    use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl, connection::SimpleConnection};
    use std::sync::Arc;
    use tls_codec::Deserialize;
    use xmtp_db::{
        ConnectionExt, StorageOption, TestDb, XmtpTestDb,
        group_message::DeliveryStatus,
        incoming_envelope::{IncomingRetry, QueryIncomingEnvelope, StreamTopic},
    };
    use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;
    use xmtp_proto::{
        api_client::ApiBuilder, prelude::XmtpTestClient, types::OrderedEnvelopeBatch,
    };
    tester!(alix, persistent_db, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let message_id =
        group.send_message_optimistic(b"rejected own application", Default::default())?;
    let (intent, mut attempt) = prepare_message(&group).await?;
    let mut envelope = ClientEnvelope::decode(attempt.envelopes[0].as_slice())?;
    let Some(Payload::GroupMessage(message)) = &mut envelope.payload else {
        panic!("expected an application envelope");
    };
    // Retain valid MLS framing but put its public header in a future epoch.
    let mut header = std::io::Cursor::new(&message.data[4..]);
    let group_id = tls_codec::VLBytes::tls_deserialize(&mut header)?;
    assert_eq!(group_id.as_slice(), group.group_id.as_slice());
    let epoch_offset = 4 + header.position() as usize;
    message.data[epoch_offset..epoch_offset + 8]
        .copy_from_slice(&(attempt.base.epoch + 1).to_be_bytes());
    attempt.payload_hash = sha256(&message.data).to_vec();
    attempt.envelopes = vec![envelope.encode_to_vec()];
    let bytes = xmtp_db::db_serialize(&attempt)?;
    let db = alix.context.db();
    db.raw_query(|conn| {
        use xmtp_db::schema::group_intents::dsl;
        diesel::update(dsl::group_intents.filter(dsl::id.eq(intent.id)))
            .set((
                dsl::payload_hash.eq(&attempt.payload_hash),
                dsl::prepared_envelopes.eq(bytes),
            ))
            .execute(conn)
    })?;
    let topic = StreamTopic::group(group.group_id);
    let before = db.topic_progress(&topic)?;
    group.publish_intents().await?;
    let wire = Topic::new_group_message(group.group_id);
    let envelopes = xmtp_common::wait_for_some(|| async {
        let envelopes = group
            .context
            .api()
            .query_all(
                [(wire.clone(), before.received)].into(),
                group.context.api().limits().max_query_limit as u32,
            )
            .await
            .ok()?;
        (envelopes.len() == 1).then_some(envelopes)
    })
    .await?;
    alix.mls_store().admit_incoming_batch(
        &OrderedEnvelopeBatch {
            topic: wire,
            after: before.received,
            envelopes,
        },
        group
            .context
            .incoming_runtime()
            .policy()
            .incoming_limits(topic.kind),
    )?;
    let received = db.topic_progress(&topic)?.received;
    let hash = alix.context.mls_storage().hash_all()?;
    let events = alix.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::MessageStatusChanged]),
        Some(8),
    );
    db.raw_query(|conn| conn.batch_execute("CREATE TRIGGER abort_rejected_echo BEFORE DELETE ON incoming_envelopes BEGIN SELECT RAISE(FAIL, 'test rollback'); END;"))?;
    assert!(matches!(
        group.process_pending_group_head(None)?,
        GroupHeadOutcome::Waiting { .. }
    ));
    let unchanged: StoredGroupIntent = db.fetch(&intent.id)?.unwrap();
    assert_eq!(unchanged.state, IntentState::Published);
    assert_eq!(
        db.get_group_message(&message_id)?.unwrap().delivery_status,
        DeliveryStatus::Unpublished
    );
    assert_eq!(db.topic_progress(&topic)?.processed, before.processed);
    assert!(db.read_last_rejection(&topic)?.is_none());
    assert_eq!(alix.context.mls_storage().hash_all()?, hash);
    assert!(events.drain().is_empty());
    db.raw_query(|conn| conn.batch_execute("DROP TRIGGER abort_rejected_echo"))?;
    db.set_incoming_retry(
        &topic,
        received,
        &IncomingRetry {
            retry_at_ns: 0,
            blocked: false,
            error_code: None,
            retry_expires_at_ns: None,
        },
    )?;
    assert!(matches!(
        group.process_pending_group_head(None)?,
        GroupHeadOutcome::Progress { result: Err(_), .. }
    ));
    let failed: StoredGroupIntent = db.fetch(&intent.id)?.unwrap();
    assert_eq!(failed.state, IntentState::Error);
    assert_eq!(
        db.get_group_message(&message_id)?.unwrap().delivery_status,
        DeliveryStatus::Failed
    );
    assert_eq!(db.topic_progress(&topic)?.processed, received);
    assert!(db.first_pending_envelope(&topic)?.is_none());
    assert_eq!(
        db.read_last_rejection(&topic)?.unwrap().code,
        "impossible_future_epoch"
    );
    assert!(
        matches!(events.drain().as_slice(), [xmtp_events::EventEnvelope { client: Some(xmtp_events::ClientEvent::MessageStatusChanged(change)), .. }] if change.message_id == message_id && change.current == xmtp_events::MessageStatus::Failed)
    );
    let assert_reason = |error: GroupError| {
        let GroupError::Sync(summary) = error else {
            panic!("expected the rejected send cause");
        };
        assert!(
            summary
                .process
                .errored
                .iter()
                .any(|(cursor, cause)| *cursor == received
                    && matches!(
                        cause,
                        GroupMessageProcessingError::RejectedIntent("impossible_future_epoch")
                            | GroupMessageProcessingError::FutureEpoch(..)
                    ))
        );
    };
    assert_reason(
        group
            .sync_until_intent_resolved(intent.id)
            .await
            .unwrap_err(),
    );
    let database = match alix.context.store().opts() {
        StorageOption::Persistent(path) => path.clone(),
        _ => panic!("expected persistent storage"),
    };
    let group_id = group.group_id;
    assert!(
        xmtp_common::wait_for_some(|| async {
            alix.context
                .incoming_runtime()
                .coordinator
                .lock()
                .is_none()
                .then_some(())
        })
        .await
        .is_some()
    );
    drop(events);
    drop(db);
    drop(group);
    drop(alix);
    let store = TestDb::create_persistent_store(Some(database)).await;
    let api = Arc::new(DefaultTestClientCreator::create().build()?);
    let restarted = Client::builder(IdentityStrategy::CachedOnly)
        .store(store)
        .api_client(api)
        .default_mls_store()?
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .with_disable_workers(true)
        .with_commit_log_worker(false)
        .with_device_sync_worker_mode(Some(DeviceSyncMode::Disabled))
        .build()
        .await?;
    let group = restarted.group(&group_id)?;
    assert_eq!(
        group
            .context
            .db()
            .get_group_message(&message_id)?
            .unwrap()
            .delivery_status,
        DeliveryStatus::Failed
    );
    assert_reason(
        group
            .sync_until_intent_resolved(intent.id)
            .await
            .unwrap_err(),
    );
    let next = group
        .send_message(b"next intent", Default::default())
        .await?;
    assert_eq!(
        group
            .context
            .db()
            .get_group_message(&next)?
            .unwrap()
            .delivery_status,
        DeliveryStatus::Published
    );
    assert_reason(
        group
            .sync_until_intent_resolved(intent.id)
            .await
            .unwrap_err(),
    );
}
