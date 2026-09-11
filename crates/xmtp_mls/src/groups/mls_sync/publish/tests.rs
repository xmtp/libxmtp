use super::*;
use crate::tester;
use prost::Message;
use xmtp_proto::types::Topic;

mod deadlines;

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
    bo.sync_welcomes().await?;
    caro.sync_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    let caro_group = caro.group(&group.group_id)?;
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
    for peer in [&group, &bo_group, &caro_group] {
        peer.receive().await?;
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
    for peer in [&group, &bo_group, &caro_group] {
        peer.receive().await?;
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

#[xmtp_common::test(unwrap_try = true)]
async fn prepared_attempt_reads_the_prior_format_without_changing_envelopes() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    group.send_message_optimistic(b"prior prepared format", Default::default())?;
    let (_, attempt) = prepare_message(&group).await?;

    // A tuple has the same bincode field layout as the prior struct.
    let prior = xmtp_db::db_serialize(&(
        1u8,
        attempt.base.clone(),
        attempt.payload_hash.clone(),
        attempt.envelopes.clone(),
        attempt.proposals.clone(),
        attempt.receipts.clone(),
        attempt.welcomes.clone(),
    ))?;
    let restored = PreparedAttempt::decode(&prior)?;
    assert_eq!(restored.version, 2);
    assert_eq!(restored, attempt);
    assert_eq!(restored.envelopes, attempt.envelopes);
    assert!(restored.same_attempt(&attempt));
    assert_eq!(
        PreparedAttempt::decode(&xmtp_db::db_serialize(&restored)?)?,
        attempt
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn rejected_intent_keeps_its_typed_cause_after_restart_and_later_rejection() {
    use crate::{
        Client,
        builder::DeviceSyncMode,
        groups::{
            EnableProposalsOptions,
            intents::{PermissionPolicyOption, PermissionUpdateType},
        },
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
        .enable_proposals(EnableProposalsOptions::test_default())
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
                GroupMessageProcessingError::CommitValidation(
                    CommitValidationError::InsufficientPermissions
                )
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
                GroupMessageProcessingError::CommitValidation(
                    CommitValidationError::InsufficientPermissions
                )
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
    use crate::groups::EnableProposalsOptions;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix.create_group(None, None)?;
    let enabled = group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await;
    assert!(
        enabled.is_ok(),
        "proposal setup failed: {:?}",
        enabled.map_err(|error| match error {
            GroupError::SyncFailedToWait(summary) => summary.publish_errors,
            error => vec![error],
        })
    );
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
        .send_group_messages(vec![attempt.publish_unit()?])
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
            xmtp_configuration::BACKEND_DEFAULT_MAX_QUERY_LIMIT as u32,
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
        .send_group_messages(vec![old_attempt.publish_unit()?])
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
    let oversized = vec![42; xmtp_configuration::BACKEND_DEFAULT_MAX_ENVELOPE_BYTES + 1];
    let failed_id = group.send_message_optimistic(&oversized, Default::default())?;
    let later_id = group.send_message_optimistic(b"after rejected request", Default::default())?;
    assert!(matches!(
        group.publish_intents().await,
        Err(GroupError::WrappedApi(xmtp_api::ApiError::EnvelopeTooLarge))
    ));

    let failed: StoredGroupMessage = group.context.db().fetch(&failed_id)?.unwrap();
    assert_eq!(failed.delivery_status, DeliveryStatus::Failed);
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
        .data(requirements.intent.data.clone())
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
    group.context.api().publish_units(welcomes.units()?).await?;

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
