use crate::groups::test_support::{EnginePeer, EngineTestGroup, EngineTestPeer};
use crate::prelude::*;

use super::*;
use std::sync::Arc;

#[xmtp_common::test(unwrap_try = true)]
async fn publish_stores_envelope_metadata_without_sync() {
    use xmtp_proto::types::Topic;

    let alix = EngineTestPeer::new().await;
    // Rotate the group key before the message exists.
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let id = group.send_message_optimistic(b"publish metadata", Default::default())?;
    group.publish_intents().await?;
    let published = alix.context.db().find_group_intents(
        group.group_id,
        Some(vec![IntentState::Published]),
        Some(vec![IntentKind::SendMessage]),
    )?;
    assert_eq!(published.len(), 1);

    // Read the local row before any query. Publishing must fill both fields.
    let stored: StoredGroupMessage = alix.context.db().fetch(&id)?.unwrap();
    assert_eq!(stored.sequence_id, 0);
    assert!(
        stored.envelope_hash.is_some(),
        "publish left envelope_hash NULL"
    );
    assert!(stored.expiry_ns.is_some(), "publish left expiry_ns NULL");

    // Read the wire envelope without processing it into the local database.
    let envelopes = alix
        .context
        .api()
        .query_all(
            [(Topic::new_group_message(group.group_id), Cursor(0))].into(),
            alix.context.api().limits().max_query_limit as u32,
        )
        .await?;
    let envelope = envelopes.last().unwrap();
    let canonical = xmtp_mls_validation::parse_envelope(envelope.envelope.clone().unwrap())?;
    assert_eq!(
        stored.envelope_hash,
        Some(canonical.canonical.hash.to_vec())
    );
    assert_eq!(
        stored.expiry_ns,
        Some(i64::try_from(envelope.meta.as_ref().unwrap().expiry_ns)?)
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn partial_envelope_metadata_preserves_stored_fields() {
    use xmtp_proto::types::Topic;

    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    let id = group
        .send_message(b"partial metadata", Default::default())
        .await?;
    let original: StoredGroupMessage = alix.context.db().fetch(&id)?.unwrap();
    assert!(original.envelope_hash.is_some());
    assert!(original.expiry_ns.is_some());
    let mut envelopes = alix
        .context
        .api()
        .query_all(
            [(Topic::new_group_message(group.group_id), Cursor(0))].into(),
            alix.context.api().limits().max_query_limit as u32,
        )
        .await?;
    let mut envelope = xmtp_api_backend::envelope::decode_group_message(envelopes.pop().unwrap())?;
    envelope.envelope_hash = None;
    EngineTestGroup::save_envelope_metadata_with_db(&alix.context.db(), &envelope)?;
    let stored: StoredGroupMessage = alix.context.db().fetch(&id)?.unwrap();
    assert_eq!(stored.envelope_hash, original.envelope_hash);
    assert_eq!(stored.expiry_ns, original.expiry_ns);

    envelope.envelope_hash = original.envelope_hash.clone();
    envelope.expiry_ns = None;
    EngineTestGroup::save_envelope_metadata_with_db(&alix.context.db(), &envelope)?;
    let stored: StoredGroupMessage = alix.context.db().fetch(&id)?.unwrap();
    assert_eq!(stored.envelope_hash, original.envelope_hash);
    assert_eq!(stored.expiry_ns, original.expiry_ns);
}

/// This test is not reproducible in webassembly, b/c webassembly has only one thread.
#[cfg_attr(
    not(target_arch = "wasm32"),
    tokio::test(flavor = "multi_thread", worker_threads = 10)
)]
#[cfg(not(target_family = "wasm"))]
async fn publish_intents_worst_case_scenario() {
    let amal_a = EngineTestPeer::new_with(|options| options.triggers = true).await;
    let amal_group_a: Arc<MlsGroup<_>> =
        Arc::new(amal_a.create_group(None, Default::default()).unwrap());

    let db = amal_a.context.db();

    // create group intent
    amal_group_a.sync().await.unwrap();
    assert_eq!(db.intents_processed(), 1);

    for _ in 0..100 {
        use crate::groups::send_message_opts::SendMessageOpts;

        let s = xmtp_common::rand_string::<100>();
        amal_group_a
            .send_message_optimistic(s.as_bytes(), SendMessageOpts::default())
            .unwrap();
    }

    let mut set = tokio::task::JoinSet::new();
    for _ in 0..50 {
        let g = amal_group_a.clone();
        set.spawn(async move { g.publish_intents().await });
    }

    let res = set.join_all().await;
    let errs: Vec<&Result<_, _>> = res.iter().filter(|r| r.is_err()).collect();
    errs.iter().for_each(|e| {
        tracing::error!("{}", e.as_ref().unwrap_err());
    });

    let published = db.intents_published();
    assert_eq!(published, 101);
    let created = db.intents_created();
    assert_eq!(created, 101);
    if !errs.is_empty() {
        panic!("Errors during publish");
    }
}

#[xmtp_common::test]
async fn hmac_keys_work_as_expected() {
    let amal = Arc::new(EngineTestPeer::new_with(|o| o.triggers = true).await);
    let amal_group: Arc<EngineTestGroup> =
        Arc::new(amal.create_group(None, Default::default()).unwrap());

    let hmac_keys = amal_group.hmac_keys(-1..=1).unwrap();
    let current_hmac_key = amal_group.hmac_keys(0..=0).unwrap().pop().unwrap();
    assert_eq!(hmac_keys.len(), 3);
    assert_eq!(hmac_keys[1].key, current_hmac_key.key);
    assert_eq!(hmac_keys[1].epoch, current_hmac_key.epoch);

    // Make sure the keys are different
    assert_ne!(hmac_keys[0].key, hmac_keys[1].key);
    assert_ne!(hmac_keys[0].key, hmac_keys[2].key);
    assert_ne!(hmac_keys[1].key, hmac_keys[2].key);

    // Make sure the epochs align
    let current_epoch = hmac_epoch();
    assert_eq!(hmac_keys[0].epoch, current_epoch - 1);
    assert_eq!(hmac_keys[1].epoch, current_epoch);
    assert_eq!(hmac_keys[2].epoch, current_epoch + 1);
}

/// A fixed root key, group id and epoch derive the same key and sender
/// HMAC as before the derivation buffers were made zeroizing. The bytes
/// were recorded from the code at `50b83f6316` and match an independent
/// RFC 5869 HKDF-SHA256 and RFC 2104 HMAC-SHA256 computation.
// verifies: PUSH-258
#[xmtp_common::test(unwrap_try = true)]
async fn hmac_derivation_matches_known_answer() {
    use xmtp_db::group::ConversationType;

    const ROOT_KEY: &str =
        "0b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83a8cdf2173c6186abd0f51a3f6489aed3f8";
    const GROUP_ID: &str = "a0a1a2a3a4a5a6a7a8a9aaabacadaeaf";
    const EPOCH: i64 = 678;
    const PAYLOAD: &[u8] = b"PUSH-258 known answer";
    const DERIVED_KEY: &str =
        "6468ea9126760f346449c020e4bd094ddea2103e6aafe3e5d66cc862e92b32799965df39f5fefaab55b7";
    const SENDER_HMAC: &str = "673228bd6e2b8ee58e74e708f4cfebdec70658703cf5a9a782f2acac9354ebf2";

    let alix = EngineTestPeer::new().await;
    let db = alix.context.db();
    xmtp_db::user_preferences::store_test_root_key(
        &db,
        &hex::decode(ROOT_KEY)?,
        xmtp_common::time::now_ns() + xmtp_common::NS_IN_DAY,
    )?;
    let group = MlsGroup::new(
        alix.context.clone(),
        GroupId::try_from(hex::decode(GROUP_ID)?.as_slice())?,
        None,
        ConversationType::Group,
        0,
    );

    // The epoch comes from the clock, so ask for the fixed one by its delta.
    let delta = EPOCH - hmac_epoch();
    let keys = group.hmac_keys_in(&db, alix.context.events(), delta..=delta)?;
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].epoch, EPOCH);
    assert_eq!(hex::encode(keys[0].key), DERIVED_KEY);

    // Any group keys: the sender HMAC does not depend on the signature.
    let group_keys = xmtp_mls_group_keys::group_keys::GroupKeys::derive(
        &xmtp_mls_group_keys::group_keys::GroupRoot::from_bytes(&[7; 32])?,
    );
    let envelopes =
        post_commit::group_message_envelopes(&keys[0], &group_keys, vec![(PAYLOAD, true)])?;
    let [
        ClientEnvelope {
            payload: Some(Payload::GroupMessage(message)),
        },
    ] = envelopes.as_slice()
    else {
        panic!("expected one group message envelope, got {envelopes:?}");
    };
    assert_eq!(message.data, PAYLOAD);
    assert_eq!(hex::encode(&message.sender_hmac), SENDER_HMAC);
}

/// A DeleteMessage that cannot be decoded, or names no message id, is
/// skipped without failing sync and stores no deletion.
#[xmtp_common::test(unwrap_try = true)]
async fn malformed_delete_messages_store_no_deletion() {
    use xmtp_content_types::{
        ContentCodec, compression::compress, delete_message::DeleteMessageCodec,
        encoded_content_to_bytes,
    };
    use xmtp_db::group_message::{
        ContentType, DeliveryStatus, GroupMessageKind, StoredGroupMessage,
    };
    use xmtp_db::message_deletion::QueryMessageDeletion;
    use xmtp_proto::xmtp::mls::message_contents::Compression;
    use xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage;

    let alix = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    let delete_of = |message_id: &str| {
        DeleteMessageCodec::encode(DeleteMessage {
            message_id: message_id.into(),
        })
    };
    let mut malformed_inner = delete_of("")?;
    malformed_inner.content = vec![0xFF, 0xFE, 0xFD];
    let oversized = compress(delete_of(&"ab".repeat(512 * 1024))?, Compression::Deflate)?;

    let rows = [
        ("malformed EncodedContent", vec![0xFF, 0xFE, 0xFD]),
        (
            "malformed DeleteMessage",
            encoded_content_to_bytes(malformed_inner),
        ),
        (
            "non-hex id",
            encoded_content_to_bytes(delete_of("not_valid_hex!!!")?),
        ),
        ("2-byte id", encoded_content_to_bytes(delete_of("0102")?)),
        (
            "33-byte id",
            encoded_content_to_bytes(delete_of(&"ab".repeat(33))?),
        ),
        (
            "compressed oversized id",
            encoded_content_to_bytes(oversized),
        ),
    ];
    let storage = alix.context.mls_storage();
    for (n, (case, decrypted_message_bytes)) in (1..).zip(rows) {
        // The receive path stores the reference it decodes from the bytes.
        let fields = EngineTestGroup::extract_queryable_content_fields(&decrypted_message_bytes);
        let message = StoredGroupMessage {
            id: vec![n; 32],
            group_id: alix_group.group_id,
            decrypted_message_bytes,
            sent_at_ns: xmtp_common::time::now_ns(),
            kind: GroupMessageKind::Application,
            sender_installation_id: vec![1, 2, 3],
            sender_inbox_id: alix.inbox_id().to_string(),
            delivery_status: DeliveryStatus::Published,
            content_type: ContentType::DeleteMessage,
            version_major: 1,
            version_minor: 0,
            authority_id: "xmtp.org".to_string(),
            reference_id: fields.reference_id,
            expiry_unknown: false,
            deleted_by_inbox_id: None,
            expire_at_ns: None,
            sequence_id: n.into(),
            envelope_hash: None,
            expiry_ns: None,
            inserted_at_ns: 0,
            should_push: false,
            idempotency_key: String::new(),
        };
        let result = xmtp_mls_core::state_tx::state_write_with_events(
            storage,
            alix.context.events(),
            |tx, buffer| {
                let storage = tx.storage();
                alix_group.store_external_application_message(&storage, &message, true, buffer)?;
                Ok::<_, GroupMessageProcessingError>(xmtp_db::TransactionOutcome::Continue(()))
            },
        );
        assert!(result.is_ok(), "{case}: {result:?}");
        let deletion = alix.context.db().get_message_deletion(&message.id)?;
        assert!(deletion.is_none(), "{case}: {deletion:?}");
    }
}

/// Pin the `CommitResult` mapping for each arm of the AppData-aware
/// wrapper error so future refactors of `commit_result()` can't
/// silently reshuffle what a receiver will write to the remote
/// commit log. In particular: `AppDataDecode` failures must be
/// `Invalid` (non-retriable wire-format violation), not
/// `Undecryptable` (retriable transport failure).
#[test]
fn process_message_with_app_data_error_commit_result_mapping() {
    use super::super::app_data::ProcessMessageWithAppDataError;
    use openmls::group::ValidationError;
    use xmtp_mls_group_state::app_data::component_source::ComponentSourceError;

    let wrong_epoch = GroupMessageProcessingError::OpenMlsProcessMessageWithAppData(
        ProcessMessageWithAppDataError::OpenMls(ProcessMessageError::ValidationError(
            ValidationError::WrongEpoch,
        )),
    );
    assert_eq!(wrong_epoch.commit_result(), CommitResult::WrongEpoch);

    let other_openmls = GroupMessageProcessingError::OpenMlsProcessMessageWithAppData(
        ProcessMessageWithAppDataError::OpenMls(ProcessMessageError::IncompatibleWireFormat),
    );
    assert_eq!(other_openmls.commit_result(), CommitResult::Undecryptable);

    // AppData decode failures are deterministic wire-format violations:
    // retrying the same bytes can't fix them, and the receiver should
    // log them as `Invalid` rather than `Undecryptable`.
    let decode_failure = GroupMessageProcessingError::OpenMlsProcessMessageWithAppData(
        ProcessMessageWithAppDataError::AppDataDecode(ComponentSourceError::UnknownComponent(
            xmtp_mls_group_state::app_data::component_id::ComponentId::from(0u16),
        )),
    );
    assert_eq!(decode_failure.commit_result(), CommitResult::Invalid);
    assert!(
        !decode_failure.is_retryable(),
        "wire-format violations must not be retriable"
    );
}
