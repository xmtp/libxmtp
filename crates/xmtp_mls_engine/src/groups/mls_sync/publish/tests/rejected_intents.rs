//! Publish tests: rejected intents.

use super::*;

#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn rejected_intent_keeps_its_typed_cause_after_restart_and_later_rejection() {
    use crate::groups::intents::{PermissionPolicyOption, PermissionUpdateType};

    use xmtp_common::wait_for_some;
    use xmtp_db::{
        StorageOption,
        incoming_envelope::{QueryIncomingEnvelope, StreamTopic},
    };

    use xmtp_mls_group_state::group_mutable_metadata::MetadataField;

    let alix = EngineTestPeer::persistent().await;
    let bo = EngineTestPeer::new().await;
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
    // Keep valid TLS framing and sign the changed bytes, so backend admission
    // reaches the client rejection path.
    *message.data.last_mut().unwrap() ^= 1;
    group.sign_as_member(message);
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
    let owner = alix.owner.clone();
    drop(alix);

    // Reopen the original file and cached identity after dropping the first client.
    let restarted = EngineTestPeer::open_persistent(
        owner,
        &database,
        xmtp_mls_identity::identity_lifecycle::IdentityStrategy::CachedOnly,
        |_| {},
    )
    .await;
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
async fn oversized_unprepared_message_does_not_block_later_intents() {
    use xmtp_db::group_message::DeliveryStatus;

    let alix = EngineTestPeer::new().await;
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

    let alix = EngineTestPeer::new().await;
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
async fn intent_sync_rejects_an_absent_intent() {
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let absent_id = i32::MAX;
    assert!(Fetch::<StoredGroupIntent>::fetch(&group.context.db(), &absent_id)?.is_none());
    assert!(matches!(
        group.sync_until_intent_resolved(absent_id).await,
        Err(GroupError::NotFound(xmtp_db::NotFound::IntentById(id))) if id == absent_id
    ));
}

// verifies: SEND-016
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn rejected_own_application_echo_fails_atomically_and_survives_restart() {
    use crate::groups::mls_sync::GroupHeadOutcome;
    use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl, connection::SimpleConnection};

    use tls_codec::Deserialize;
    use xmtp_db::{
        ConnectionExt, StorageOption,
        group_message::DeliveryStatus,
        incoming_envelope::{IncomingRetry, QueryIncomingEnvelope, StreamTopic},
    };

    use xmtp_proto::types::OrderedEnvelopeBatch;
    let alix = EngineTestPeer::persistent().await;
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
    // Sign the changed bytes, so backend admission accepts them.
    group.sign_as_member(message);
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
            envelopes: envelopes.into_iter().map(Into::into).collect(),
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
    let owner = alix.owner.clone();
    drop(alix);
    let restarted = EngineTestPeer::open_persistent(
        owner,
        &database,
        xmtp_mls_identity::identity_lifecycle::IdentityStrategy::CachedOnly,
        |_| {},
    )
    .await;
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
