use super::*;

fn transcript_content(type_id: &str, version_major: u32, authority: &str) -> Vec<u8> {
    use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};
    EncodedContent {
        r#type: Some(ContentTypeId {
            authority_id: authority.into(),
            type_id: type_id.into(),
            version_major,
            version_minor: 3,
        }),
        content: b"fixture".to_vec(),
        ..Default::default()
    }
    .encode_to_vec()
}

fn legacy_stored_message<Context: XmtpSharedContext>(
    group: &MlsGroup<Context>,
    content: &[u8],
    key: &str,
) -> Result<Vec<u8>, GroupError> {
    use xmtp_db::Store;
    let template_id = group.prepare_message_for_later_publish(
        b"legacy fixture template",
        false,
        Some(format!("template-{key}")),
    )?;
    let db = group.context.db();
    let mut row = db.get_group_message(&template_id)?.unwrap();
    row.id = calculate_message_id(group.group_id, content, key);
    row.decrypted_message_bytes = content.to_vec();
    row.idempotency_key = key.into();
    row.store(&db)?;
    Ok(row.id)
}

fn legacy_message_intent<Context: XmtpSharedContext>(
    group: &MlsGroup<Context>,
    content: &[u8],
    key: &str,
) -> Result<StoredGroupIntent, GroupError> {
    let envelope = PlaintextEnvelope {
        content: Some(Content::V1(V1 {
            content: content.to_vec(),
            idempotency_key: key.into(),
        })),
    };
    let data: Vec<u8> = SendMessageIntentData::new(envelope.encode_to_vec()).into();
    QueueIntent::send_message().data(data).queue(group)
}

async fn legacy_saved_attempt<Context: XmtpSharedContext>(
    group: &MlsGroup<Context>,
    content: &[u8],
    key: &str,
) -> Result<(Vec<u8>, StoredGroupIntent, PreparedAttempt), GroupError> {
    let id = legacy_stored_message(group, content, key)?;
    let intent = legacy_message_intent(group, content, key)?;
    let requirements = crate::state_tx::state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls_group, _| {
            PublishRequirements::capture(mls_group, &intent).map(Continue)
        })
    })?
    .into_continued();
    let mut dependencies = group.resolve_publish_dependencies(&requirements).await?;
    let attempt = group
        .prepare_reserved_attempt_fixture(&requirements, &mut dependencies)?
        .ok_or(GroupError::UninitializedResult)?;
    Ok((id, intent, attempt))
}

// verifies: GMOD-035
#[xmtp_common::test(unwrap_try = true)]
async fn transcript_types_are_never_sent() {
    use crate::groups::send_message_opts::SendMessageOpts;
    use xmtp_events::{ClientEvent, EventFilter, EventKind, MessageStatus};
    use xmtp_proto::api::HasStats;

    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    for type_id in ["group_updated", "group_membership_change"] {
        for version in [1, 7] {
            let content = transcript_content(type_id, version, "xmtp.org");
            let initial_messages = group.find_messages(&Default::default())?.len();
            let initial_intents = group
                .context
                .db()
                .find_group_intents(group.group_id, None, Some(IntentKind::all().collect()))?
                .len();
            let before = alix
                .context
                .api()
                .api_client
                .as_ref()
                .mls_stats()
                .publish
                .get_count();
            assert!(matches!(
                group
                    .send_message(&content, SendMessageOpts::default())
                    .await,
                Err(GroupError::ReservedTranscriptContentType)
            ));
            assert!(matches!(
                group.send_message_optimistic(&content, SendMessageOpts::default()),
                Err(GroupError::ReservedTranscriptContentType)
            ));
            for _ in 0..2 {
                assert!(matches!(
                    group.prepare_message_for_later_publish(
                        &content,
                        false,
                        Some(format!("{type_id}-{version}"))
                    ),
                    Err(GroupError::ReservedTranscriptContentType)
                ));
            }
            assert_eq!(
                alix.context
                    .api()
                    .api_client
                    .as_ref()
                    .mls_stats()
                    .publish
                    .get_count(),
                before
            );
            assert_eq!(
                group.find_messages(&Default::default())?.len(),
                initial_messages
            );
            assert_eq!(
                group
                    .context
                    .db()
                    .find_group_intents(group.group_id, None, Some(IntentKind::all().collect()),)?
                    .len(),
                initial_intents
            );

            let key = format!("old-stored-{type_id}-{version}");
            let stored_id = legacy_stored_message(&group, &content, &key)?;
            assert!(matches!(
                group.prepare_message_for_later_publish(&content, false, Some(key.clone())),
                Err(GroupError::ReservedTranscriptContentType)
            ));
            let statuses = alix.context.events().subscribe(
                EventFilter::new([EventKind::MessageStatusChanged]),
                Some(10),
            );
            let before = alix
                .context
                .api()
                .api_client
                .as_ref()
                .mls_stats()
                .publish
                .get_count();
            assert!(matches!(
                group.publish_stored_message(&stored_id).await,
                Err(GroupError::ReservedTranscriptContentType)
            ));
            assert_eq!(
                alix.context
                    .api()
                    .api_client
                    .as_ref()
                    .mls_stats()
                    .publish
                    .get_count(),
                before
            );
            assert_eq!(
                group
                    .context
                    .db()
                    .get_group_message(&stored_id)?
                    .unwrap()
                    .delivery_status,
                DeliveryStatus::Failed
            );
            assert!(matches!(
                statuses.drain().as_slice(),
                [xmtp_events::EventEnvelope {
                    client: Some(ClientEvent::MessageStatusChanged(change)), ..
                }] if change.message_id == stored_id
                    && change.previous == MessageStatus::Unpublished
                    && change.current == MessageStatus::Failed
            ));
            assert!(matches!(
                group.publish_stored_message(&stored_id).await,
                Err(GroupError::ReservedTranscriptContentType)
            ));
            assert!(statuses.drain().is_empty());

            let key = format!("old-queued-{type_id}-{version}");
            let queued_id = legacy_stored_message(&group, &content, &key)?;
            let intent = legacy_message_intent(&group, &content, &key)?;
            let before = alix
                .context
                .api()
                .api_client
                .as_ref()
                .mls_stats()
                .publish
                .get_count();
            assert!(matches!(
                group.sync_until_intent_resolved(intent.id).await,
                Err(GroupError::ReservedTranscriptContentType)
            ));
            assert_eq!(
                alix.context
                    .api()
                    .api_client
                    .as_ref()
                    .mls_stats()
                    .publish
                    .get_count(),
                before
            );
            let current: StoredGroupIntent = group.context.db().fetch(&intent.id)?.unwrap();
            assert_eq!(current.state, IntentState::Error);
            assert_eq!(
                group.context.db().local_intent_rejection_reason(intent.id)?,
                Some(xmtp_db::group_intent::LocalIntentRejectionReason::ReservedTranscriptContentType)
            );
            assert_eq!(
                group
                    .context
                    .db()
                    .get_group_message(&queued_id)?
                    .unwrap()
                    .delivery_status,
                DeliveryStatus::Failed
            );
            assert!(matches!(
                statuses.drain().as_slice(),
                [xmtp_events::EventEnvelope {
                    client: Some(ClientEvent::MessageStatusChanged(change)), ..
                }] if change.message_id == queued_id
                    && change.previous == MessageStatus::Unpublished
                    && change.current == MessageStatus::Failed
            ));
            assert!(matches!(
                group.publish_stored_message(&queued_id).await,
                Err(GroupError::ReservedTranscriptContentType)
            ));
            assert!(statuses.drain().is_empty());
        }
    }

    let content = transcript_content("group_updated", 1, "xmtp.org");
    let old_id = legacy_stored_message(&group, &content, "old-unknown-error")?;
    let old_intent = legacy_message_intent(&group, &content, "old-unknown-error")?;
    group.context.db().set_group_intent_error(old_intent.id)?;
    assert!(matches!(
        group.publish_stored_message(&old_id).await,
        Err(GroupError::Sync(_))
    ));
    assert_eq!(
        group
            .context
            .db()
            .local_intent_rejection_reason(old_intent.id)?,
        None
    );

    // The type id alone is not reserved for another authority.
    let allowed = transcript_content("group_updated", 1, "example.org");
    group
        .send_message(&allowed, SendMessageOpts::default())
        .await?;
    // Only the outer type controls the guard, even for compressed content.
    let mut compressed = xmtp_proto::xmtp::mls::message_contents::EncodedContent::decode(
        transcript_content("group_membership_change", 7, "xmtp.org").as_slice(),
    )?;
    compressed.compression = Some(1);
    assert!(matches!(
        group
            .send_message(&compressed.encode_to_vec(), SendMessageOpts::default())
            .await,
        Err(GroupError::ReservedTranscriptContentType)
    ));
    let mut reply = xmtp_proto::xmtp::mls::message_contents::EncodedContent::decode(
        transcript_content("reply", 1, "xmtp.org").as_slice(),
    )?;
    reply.content = transcript_content("group_updated", 7, "xmtp.org");
    group
        .send_message(&reply.encode_to_vec(), SendMessageOpts::default())
        .await?;
    group
        .send_message(b"ordinary bytes", SendMessageOpts::default())
        .await?;
}

// verifies: GMOD-035
#[xmtp_common::test(unwrap_try = true)]
async fn reserved_transcript_send_leaves_pending_proposal_untouched() {
    use crate::groups::{
        intents::ProposeMemberUpdateIntentData, send_message_opts::SendMessageOpts,
    };
    use xmtp_proto::api::HasStats;

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    tester!(caro, disable_workers);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let db = group.context.db();
    let intent = db.insert_group_intent(xmtp_db::group_intent::NewGroupIntent::new(
        IntentKind::ProposeMemberUpdate,
        group.group_id,
        ProposeMemberUpdateIntentData::new(vec![caro.inbox_id().to_string()], vec![]).try_into()?,
        false,
    ))?;
    group.sync_until_intent_resolved(intent.id).await?;
    let pending = group.with_group_snapshot(|openmls_group| {
        Ok::<_, GroupError>(openmls_group.pending_proposals().count())
    })?;
    assert!(pending > 0);
    let intents_before = db
        .find_group_intents(group.group_id, None, Some(IntentKind::all().collect()))?
        .len();
    let calls_before = alix
        .context
        .api()
        .api_client
        .as_ref()
        .mls_stats()
        .publish
        .get_count();
    let content = transcript_content("group_updated", 1, "xmtp.org");
    assert!(matches!(
        group
            .send_message(&content, SendMessageOpts::default())
            .await,
        Err(GroupError::ReservedTranscriptContentType)
    ));
    assert_eq!(
        alix.context
            .api()
            .api_client
            .as_ref()
            .mls_stats()
            .publish
            .get_count(),
        calls_before
    );
    assert_eq!(
        db.find_group_intents(group.group_id, None, Some(IntentKind::all().collect()))?
            .len(),
        intents_before
    );
    assert_eq!(
        group.with_group_snapshot(|openmls_group| {
            Ok::<_, GroupError>(openmls_group.pending_proposals().count())
        })?,
        pending
    );
}

// verifies: GMOD-035
#[xmtp_common::test(unwrap_try = true)]
async fn reserved_local_rejection_survives_restart_for_exact_target() {
    use xmtp_proto::api::HasStats;

    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let content = transcript_content("group_updated", 7, "xmtp.org");
    let message_id = legacy_stored_message(&group, &content, "restart-rejection")?;
    let intent = legacy_message_intent(&group, &content, "restart-rejection")?;
    assert!(matches!(
        group.publish_intents().await,
        Err(GroupError::ReservedTranscriptContentType)
    ));
    let unrelated = legacy_message_intent(&group, b"unrelated", "other-error")?;
    group.context.db().set_group_intent_error(unrelated.id)?;
    let group_id = group.group_id;
    let snapshot = std::sync::Arc::new(alix.db_snapshot());
    drop(group);
    drop(alix);

    tester!(restarted, snapshot: snapshot, disable_workers);
    let group = restarted.group(&group_id)?;
    let before = restarted
        .context
        .api()
        .api_client
        .as_ref()
        .mls_stats()
        .publish
        .get_count();
    assert!(matches!(
        group.sync_until_intent_resolved(intent.id).await,
        Err(GroupError::ReservedTranscriptContentType)
    ));
    assert!(matches!(
        group.publish_stored_message(&message_id).await,
        Err(GroupError::ReservedTranscriptContentType)
    ));
    assert_eq!(
        restarted
            .context
            .api()
            .api_client
            .as_ref()
            .mls_stats()
            .publish
            .get_count(),
        before
    );
    assert_eq!(
        group
            .context
            .db()
            .local_intent_rejection_reason(unrelated.id)?,
        None
    );
}

// verifies: GMOD-035
#[xmtp_common::test(unwrap_try = true)]
async fn reserved_queued_rejection_rolls_back_when_message_update_fails() {
    use xmtp_db::ConnectionExt;
    use xmtp_db::diesel::connection::SimpleConnection;
    use xmtp_events::{EventFilter, EventKind};
    use xmtp_proto::api::HasStats;

    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let content = transcript_content("group_updated", 1, "xmtp.org");
    let message_id = legacy_stored_message(&group, &content, "rollback-rejection")?;
    let intent = legacy_message_intent(&group, &content, "rollback-rejection")?;
    let db = group.context.db();
    let statuses = alix.context.events().subscribe(
        EventFilter::new([EventKind::MessageStatusChanged]),
        Some(10),
    );
    let before = alix
        .context
        .api()
        .api_client
        .as_ref()
        .mls_stats()
        .publish
        .get_count();
    db.raw_query(|conn| {
        conn.batch_execute(
            "CREATE TRIGGER fail_reserved_status BEFORE UPDATE OF delivery_status ON group_messages \
             BEGIN SELECT RAISE(ABORT, 'injected message status failure'); END;",
        )
    })?;
    let failure = group.publish_intents().await;
    assert!(matches!(failure, Err(GroupError::Db(_))), "{failure:?}");
    assert_eq!(
        Fetch::<StoredGroupIntent>::fetch(&db, &intent.id)?
            .unwrap()
            .state,
        IntentState::ToPublish
    );
    assert_eq!(db.local_intent_rejection_reason(intent.id)?, None);
    assert_eq!(
        db.get_group_message(&message_id)?.unwrap().delivery_status,
        DeliveryStatus::Unpublished
    );
    assert!(statuses.drain().is_empty());
    assert_eq!(
        alix.context
            .api()
            .api_client
            .as_ref()
            .mls_stats()
            .publish
            .get_count(),
        before
    );
    db.raw_query(|conn| conn.batch_execute("DROP TRIGGER fail_reserved_status;"))?;
    assert!(matches!(
        group.sync_until_intent_resolved(intent.id).await,
        Err(GroupError::ReservedTranscriptContentType)
    ));
    assert_eq!(
        Fetch::<StoredGroupIntent>::fetch(&db, &intent.id)?
            .unwrap()
            .state,
        IntentState::Error
    );
    assert_eq!(
        db.get_group_message(&message_id)?.unwrap().delivery_status,
        DeliveryStatus::Failed
    );
    assert_eq!(statuses.drain().len(), 1);
}

// verifies: GMOD-035
#[xmtp_common::test(unwrap_try = true)]
async fn reserved_saved_attempt_does_not_retry_and_late_echo_resolves() {
    use xmtp_events::{EventFilter, EventKind};
    use xmtp_proto::api::HasStats;

    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let content = transcript_content("group_membership_change", 7, "xmtp.org");
    let (message_id, intent, attempt) =
        legacy_saved_attempt(&group, &content, "accepted-without-receipt").await?;
    let original = group.context.db().prepared_envelopes(intent.id)?.unwrap();
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
    let statuses = restarted.context.events().subscribe(
        EventFilter::new([EventKind::MessageStatusChanged]),
        Some(10),
    );
    let before = restarted
        .context
        .api()
        .api_client
        .as_ref()
        .mls_stats()
        .publish
        .get_count();
    group.publish_intents().await?;
    assert_eq!(
        restarted
            .context
            .api()
            .api_client
            .as_ref()
            .mls_stats()
            .publish
            .get_count(),
        before
    );
    assert_eq!(
        group.context.db().prepared_envelopes(intent.id)?.unwrap(),
        original
    );
    assert_eq!(
        group
            .context
            .db()
            .get_group_message(&message_id)?
            .unwrap()
            .delivery_status,
        DeliveryStatus::Unpublished
    );
    group.receive().await?;
    group.sync_until_intent_resolved(intent.id).await?;
    let current: StoredGroupIntent = group.context.db().fetch(&intent.id)?.unwrap();
    assert_eq!(current.state, IntentState::Processed);
    assert_eq!(
        group
            .context
            .db()
            .get_group_message(&message_id)?
            .unwrap()
            .delivery_status,
        DeliveryStatus::Published
    );
    assert!(statuses.drain().iter().all(|event| !matches!(
        &event.client,
        Some(xmtp_events::ClientEvent::MessageStatusChanged(change))
            if change.message_id == message_id && change.current == xmtp_events::MessageStatus::Failed
    )));
}

// verifies: GMOD-035
#[xmtp_common::test(unwrap_try = true)]
async fn reserved_saved_attempt_with_receipt_does_not_retry() {
    use xmtp_common::time::Duration;
    use xmtp_proto::api::HasStats;

    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let content = transcript_content("group_updated", 7, "xmtp.org");
    let (message_id, intent, attempt) =
        legacy_saved_attempt(&group, &content, "accepted-with-receipt").await?;
    let receipts = group
        .context
        .api()
        .send_group_messages(vec![attempt.publish_unit(group.context.api().limits())?])
        .await?;
    group.record_publish_receipts(&intent, &attempt, receipts)?;
    let original = group.context.db().prepared_envelopes(intent.id)?.unwrap();
    assert!(group.published_intent_target(intent.id)?.is_some());
    let group_id = group.group_id;
    let snapshot = std::sync::Arc::new(alix.db_snapshot());
    drop(group);
    drop(alix);

    tester!(restarted, snapshot: snapshot, disable_workers);
    let group = restarted.group(&group_id)?;
    let before = restarted
        .context
        .api()
        .api_client
        .as_ref()
        .mls_stats()
        .publish
        .get_count();
    group.publish_intents().await?;
    assert_eq!(
        restarted
            .context
            .api()
            .api_client
            .as_ref()
            .mls_stats()
            .publish
            .get_count(),
        before
    );
    assert_eq!(
        group.context.db().prepared_envelopes(intent.id)?.unwrap(),
        original
    );
    assert!(group.published_intent_target(intent.id)?.is_some());
    let mut policy = restarted.context.incoming_runtime().policy().clone();
    policy.barrier_timeout = Duration::ZERO;
    let impatient = crate::builder::ClientBuilder::from_client(restarted.client.clone())
        .with_disable_workers(true)
        .with_allow_offline(Some(true))
        .stream_policy(policy)
        .build()
        .await?;
    let (impatient_group, _) = MlsGroup::new_cached(impatient.context.clone(), &group_id)?;
    assert!(matches!(
        impatient_group.sync_until_intent_resolved(intent.id).await,
        Err(GroupError::PublishedButUnconfirmed { intent_id, .. }) if intent_id == intent.id
    ));
    drop(impatient_group);
    drop(impatient);
    group.receive().await?;
    group.sync_until_intent_resolved(intent.id).await?;
    assert_eq!(
        group
            .context
            .db()
            .get_group_message(&message_id)?
            .unwrap()
            .delivery_status,
        DeliveryStatus::Published
    );
}

// verifies: GMOD-035
#[xmtp_common::test(unwrap_try = true)]
async fn reserved_unsent_saved_attempt_has_unknown_outcome_after_restart() {
    use xmtp_proto::api::HasStats;

    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let content = transcript_content("group_updated", 7, "xmtp.org");
    let (message_id, intent, _) = legacy_saved_attempt(&group, &content, "never-submitted").await?;
    let original = group.context.db().prepared_envelopes(intent.id)?.unwrap();
    let group_id = group.group_id;
    let snapshot = std::sync::Arc::new(alix.db_snapshot());
    drop(group);
    drop(alix);

    tester!(restarted, snapshot: snapshot, disable_workers);
    let group = restarted.group(&group_id)?;
    let before = restarted
        .context
        .api()
        .api_client
        .as_ref()
        .mls_stats()
        .publish
        .get_count();
    group.publish_intents().await?;
    assert_eq!(
        restarted
            .context
            .api()
            .api_client
            .as_ref()
            .mls_stats()
            .publish
            .get_count(),
        before
    );
    assert_eq!(
        group.context.db().prepared_envelopes(intent.id)?.unwrap(),
        original
    );
    assert!(matches!(
        group.publish_stored_message(&message_id).await,
        Err(GroupError::SendOutcomeUnknown { intent_id }) if intent_id == intent.id
    ));
    assert!(matches!(
        group.sync_until_intent_resolved(intent.id).await,
        Err(GroupError::SendOutcomeUnknown { intent_id }) if intent_id == intent.id
    ));
    assert_eq!(
        restarted
            .context
            .api()
            .api_client
            .as_ref()
            .mls_stats()
            .publish
            .get_count(),
        before
    );
    assert_eq!(
        group
            .context
            .db()
            .get_group_message(&message_id)?
            .unwrap()
            .delivery_status,
        DeliveryStatus::Unpublished
    );
    let allowed_id = group.send_message_optimistic(b"later allowed message", Default::default())?;
    group.publish_intents().await?;
    group.receive().await?;
    assert_eq!(
        group
            .context
            .db()
            .get_group_message(&allowed_id)?
            .unwrap()
            .delivery_status,
        DeliveryStatus::Published
    );
    let snapshot = std::sync::Arc::new(restarted.db_snapshot());
    drop(group);
    drop(restarted);
    tester!(restarted_again, snapshot: snapshot, disable_workers);
    let group = restarted_again.group(&group_id)?;
    let before = restarted_again
        .context
        .api()
        .api_client
        .as_ref()
        .mls_stats()
        .publish
        .get_count();
    group.publish_intents().await?;
    assert_eq!(
        restarted_again
            .context
            .api()
            .api_client
            .as_ref()
            .mls_stats()
            .publish
            .get_count(),
        before
    );
    assert!(matches!(
        group.sync_until_intent_resolved(intent.id).await,
        Err(GroupError::SendOutcomeUnknown { intent_id }) if intent_id == intent.id
    ));
}

// verifies: GMOD-035, SEND-019
#[xmtp_common::test(unwrap_try = true)]
async fn bulk_publish_reports_older_reserved_unknown_after_later_message_completes() {
    use xmtp_proto::api::HasStats;

    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    group.update_installations().await?;
    let content = transcript_content("group_updated", 7, "xmtp.org");
    let (reserved_id, reserved_intent, _) =
        legacy_saved_attempt(&group, &content, "bulk-reserved").await?;
    let saved = group
        .context
        .db()
        .prepared_envelopes(reserved_intent.id)?
        .unwrap();
    let allowed_id = group.send_message_optimistic(b"later allowed", Default::default())?;
    let before = alix
        .context
        .api()
        .api_client
        .as_ref()
        .mls_stats()
        .publish
        .get_count();

    let result = group.publish_messages().await;
    assert!(
        matches!(result, Err(GroupError::SendOutcomeUnknown { intent_id }) if intent_id == reserved_intent.id),
        "{result:?}"
    );
    assert_eq!(
        alix.context
            .api()
            .api_client
            .as_ref()
            .mls_stats()
            .publish
            .get_count(),
        before + 1,
        "only the later allowed message may publish"
    );
    assert_eq!(
        group
            .context
            .db()
            .get_group_message(&allowed_id)?
            .unwrap()
            .delivery_status,
        DeliveryStatus::Published
    );
    assert_eq!(
        group
            .context
            .db()
            .get_group_message(&reserved_id)?
            .unwrap()
            .delivery_status,
        DeliveryStatus::Unpublished
    );
    assert_eq!(
        Fetch::<StoredGroupIntent>::fetch(&group.context.db(), &reserved_intent.id)?
            .unwrap()
            .state,
        IntentState::Published
    );
    assert_eq!(
        group
            .context
            .db()
            .prepared_envelopes(reserved_intent.id)?
            .unwrap(),
        saved
    );
    assert!(matches!(
        group.sync_until_intent_resolved(reserved_intent.id).await,
        Err(GroupError::SendOutcomeUnknown { intent_id }) if intent_id == reserved_intent.id
    ));
}

// verifies: GMOD-035
#[xmtp_common::test(unwrap_try = true)]
async fn bulk_publish_ignores_historical_local_rejection() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    group.update_installations().await?;
    let content = transcript_content("group_membership_change", 1, "xmtp.org");
    let rejected_id = legacy_stored_message(&group, &content, "old-bulk-error")?;
    let rejected = legacy_message_intent(&group, &content, "old-bulk-error")?;
    let first_allowed = group.send_message_optimistic(b"first allowed", Default::default())?;
    assert!(matches!(
        group.publish_messages().await,
        Err(GroupError::ReservedTranscriptContentType)
    ));
    assert_eq!(
        group
            .context
            .db()
            .local_intent_rejection_reason(rejected.id)?,
        Some(xmtp_db::group_intent::LocalIntentRejectionReason::ReservedTranscriptContentType)
    );
    assert_eq!(
        group
            .context
            .db()
            .get_group_message(&rejected_id)?
            .unwrap()
            .delivery_status,
        DeliveryStatus::Failed
    );
    assert_eq!(
        group
            .context
            .db()
            .get_group_message(&first_allowed)?
            .unwrap()
            .delivery_status,
        DeliveryStatus::Published
    );
    let allowed_id = group.send_message_optimistic(b"new allowed", Default::default())?;
    group.publish_messages().await?;
    assert_eq!(
        group
            .context
            .db()
            .get_group_message(&allowed_id)?
            .unwrap()
            .delivery_status,
        DeliveryStatus::Published
    );
}

// verifies: GMOD-035, SEND-019
#[xmtp_common::test(unwrap_try = true)]
async fn bulk_publish_reports_selected_terminal_ordered_rejection_after_later_progress() {
    use xmtp_proto::api::HasStats;

    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    group.update_installations().await?;
    let content = transcript_content("group_updated", 7, "xmtp.org");
    let (reserved_id, reserved_intent, _) =
        legacy_saved_attempt(&group, &content, "bulk-ordered-rejection").await?;
    group
        .publish_future_epoch_reserved_echo_for_test(reserved_intent.id)
        .await?;
    let allowed_id = group.send_message_optimistic(b"later allowed", Default::default())?;
    let before = alix
        .context
        .api()
        .api_client
        .as_ref()
        .mls_stats()
        .publish
        .get_count();

    let result = group.publish_messages().await;
    let Err(GroupError::Sync(summary)) = result else {
        panic!("selected terminal rejection was lost: {result:?}");
    };
    assert!(summary.is_errored());
    assert_eq!(
        summary.rejected_intent_code(),
        Some("impossible_future_epoch")
    );
    assert!(matches!(
        summary.process.errored.as_slice(),
        [(
            _,
            GroupMessageProcessingError::RejectedIntent("impossible_future_epoch")
        )]
    ));
    assert_eq!(
        alix.context
            .api()
            .api_client
            .as_ref()
            .mls_stats()
            .publish
            .get_count(),
        before + 1,
        "the saved reserved attempt must not publish again"
    );
    assert_eq!(
        Fetch::<StoredGroupIntent>::fetch(&group.context.db(), &reserved_intent.id)?
            .unwrap()
            .state,
        IntentState::Error
    );
    assert_eq!(
        group
            .context
            .db()
            .local_intent_rejection_reason(reserved_intent.id)?,
        None
    );
    let saved = group
        .context
        .db()
        .prepared_envelopes(reserved_intent.id)?
        .unwrap();
    assert!(PreparedAttempt::decode(&saved)?.rejection.is_some());
    assert_eq!(
        group
            .context
            .db()
            .get_group_message(&reserved_id)?
            .unwrap()
            .delivery_status,
        DeliveryStatus::Failed
    );
    assert_eq!(
        group
            .context
            .db()
            .get_group_message(&allowed_id)?
            .unwrap()
            .delivery_status,
        DeliveryStatus::Published
    );
    assert!(matches!(
        group.sync_until_intent_resolved(reserved_intent.id).await,
        Err(GroupError::Sync(exact))
            if exact.rejected_intent_code() == Some("impossible_future_epoch")
                && matches!(
                exact.process.errored.as_slice(),
                [(_, GroupMessageProcessingError::RejectedIntent("impossible_future_epoch"))]
            )
    ));
    let later_id = group.send_message_optimistic(b"still allowed", Default::default())?;
    group.publish_messages().await?;
    assert_eq!(
        group
            .context
            .db()
            .get_group_message(&later_id)?
            .unwrap()
            .delivery_status,
        DeliveryStatus::Published
    );
}
