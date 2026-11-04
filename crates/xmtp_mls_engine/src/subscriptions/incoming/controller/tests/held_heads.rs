//! Incoming controller tests: held heads.

use super::*;

/// Held group heads cannot drain, so they are charged to the held allowance
/// and neither stop nor keep paused the receipt of unrelated groups.
// verifies: PROC-020
#[xmtp_common::test(unwrap_try = true)]
async fn held_group_heads_do_not_pause_unrelated_groups() {
    use xmtp_db::incoming_envelope::IncomingRetry;

    let alix = EngineTestPeer::new().await;
    let client = alix
        .with_stream_policy(|settings| settings.group_pending.rows = 2)
        .await;
    let first = Topic::new_group_message(GroupId::generate());
    let second = Topic::new_group_message(GroupId::generate());
    let third = Topic::new_group_message(GroupId::generate());
    let mut controller = controller(client.context.clone());
    for (id, topic) in [&first, &second, &third].into_iter().enumerate() {
        add_scope(&mut controller, id as u64 + 1, topic);
    }
    controller.transport.requested = [first.clone(), second.clone(), third.clone()].into();
    controller.transport.state = TransportState::Streaming(IncomingSubscription::new(
        Box::pin(futures::stream::pending()),
        |_| {},
    ));
    let admit = |controller: &mut Controller<_>, topic: &Topic, sequence: u64| {
        controller.incoming(Some(Ok(IncomingEvent::OrderedBatch(
            OrderedEnvelopeBatch {
                topic: topic.clone(),
                after: Cursor(sequence - 10),
                envelopes: vec![ReceivedEnvelope::from(wire::ServerEnvelope {
                    meta: Some(meta(topic, sequence)),
                    envelope: Some(wire::ClientEnvelope::default()),
                })],
            },
        ))));
    };
    admit(&mut controller, &first, 10);
    admit(&mut controller, &second, 10);
    admit(&mut controller, &third, 10);
    assert!(
        controller.receipt(&third).paused,
        "ready work fills the budget"
    );
    // Both heads become held, like commits that cannot be processed.
    for topic in [&first, &second] {
        assert!(client.context.db().set_incoming_retry(
            &topic_key(topic)?,
            Cursor(10),
            &IncomingRetry {
                retry_at_ns: 0,
                blocked: true,
                error_code: Some("group_head_blocked".into()),
                retry_expires_at_ns: None,
            },
        )?);
    }
    controller.reconcile()?;
    assert!(
        !controller.receipt(&third).paused,
        "held heads do not keep an unrelated group paused"
    );
    admit(&mut controller, &third, 10);
    assert!(!controller.receipt(&third).paused);
    assert_eq!(
        client
            .context
            .db()
            .topic_progress(&topic_key(&third)?)?
            .received,
        Cursor(10)
    );
    admit(&mut controller, &first, 20);
    assert!(
        controller.receipt(&first).paused,
        "a held topic pauses when the held allowance is full"
    );
    controller.reconcile()?;
    assert!(
        controller.receipt(&first).paused,
        "a held topic stays paused while the held allowance is full"
    );
}

/// Admission stores an unreadable Welcome variant as received and holds it.
// verifies: PROC-002, JOIN-087
#[xmtp_common::test(unwrap_try = true)]
async fn an_unknown_welcome_variant_is_stored_as_received_and_held() {
    /// A later schema that defines `WelcomeMessage` tag 3.
    #[derive(Clone, PartialEq, prost::Message)]
    struct Future {
        #[prost(message, optional, tag = "2")]
        envelope: Option<FutureClient>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    struct FutureClient {
        #[prost(message, optional, tag = "2")]
        welcome: Option<FutureWelcome>,
    }
    #[derive(Clone, PartialEq, prost::Message)]
    struct FutureWelcome {
        #[prost(bytes = "vec", tag = "3")]
        variant: Vec<u8>,
    }
    let alix = EngineTestPeer::persistent().await;
    let topic = Topic::new_welcome_message(alix.context.installation_id());
    let key = topic_key(&topic)?;
    let raw =
        |sequence| xmtp_proto::types::future_welcome_envelope(&meta(&topic, sequence), &[9; 40]);
    let decoded = wire::ServerEnvelope::decode(raw(7))?.encoded_len();
    // Two decoded envelopes fit one chunk; two received envelopes need two.
    assert!(raw(7).len() <= 2 * decoded && 2 * decoded < raw(7).len() + raw(8).len());
    let client = alix
        .with_stream_policy(|settings| {
            settings.max_admission_bytes = 2 * decoded as u64;
        })
        .await;
    let mut controller = controller(client.context.clone());
    controller.admit_received_batch(OrderedEnvelopeBatch {
        topic: topic.clone(),
        after: Cursor(0),
        envelopes: [7, 8]
            .map(|sequence| ReceivedEnvelope::from_raw(raw(sequence)))
            .into_iter()
            .collect::<Result<_, _>>()?,
    })?;
    let db = alix.context.db();
    assert_eq!(db.topic_progress(&key)?.received, Cursor(8));
    assert_eq!(db.pending_envelope(&key, Cursor(7))??.envelope, raw(7));
    assert_eq!(db.pending_envelope(&key, Cursor(8))??.envelope, raw(8));

    crate::groups::welcome_sync::WelcomeService::new(alix.context.clone())
        .process_pending_welcomes_once(&[])?;
    let held = db.pending_envelope(&key, Cursor(7))??;
    assert!(held.blocked && held.unreadable_expires_at_ns.is_some());
    assert_eq!(held.error_code.as_deref(), Some("unsupported_welcome"));

    {
        use xmtp_db::ConnectionExt;
        alix.context.db().disconnect()?;
        alix.context.db().reconnect()?;
    }
    let stored = alix
        .context
        .db()
        .pending_envelope(&key, Cursor(7))??
        .envelope;
    assert_eq!(stored, raw(7));
    let stored_meta = wire::ServerEnvelope::decode(stored.as_slice())?.meta?;
    assert_eq!(stored_meta.message_hash, meta(&topic, 7).message_hash);
    let future = Future::decode(stored.as_slice())?.envelope?.welcome?;
    assert_eq!(future.variant, [9; 40]);
}

#[xmtp_common::test(unwrap_try = true)]
async fn an_invalid_supported_head_does_not_hold_a_later_valid_message() {
    use crate::groups::{
        app_data::ProcessMessageWithAppDataError,
        mls_sync::{GroupHeadOutcome, GroupMessageProcessingError},
        send_message_opts::SendMessageOpts,
    };
    use openmls::{
        framing::errors::{MessageDecryptionError, SecretTreeError},
        group::{ProcessMessageError, ValidationError},
    };
    use xmtp_api::PublishUnit;
    use xmtp_db::group_message::MsgQueryArgs;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.find_groups(GroupQueryArgs::default())?.remove(0);
    bo_group.receive().await?;
    let topic = Topic::new_group_message(bo_group.group_id);
    let key = topic_key(&topic)?;
    let before = bo.context.db().topic_progress(&key)?;
    let message_count = bo_group.find_messages(&MsgQueryArgs::default())?.len();
    // Independent traffic leaves a sparse sequence for the malformed head.
    let caro = EngineTestPeer::new().await;
    assert_ne!(caro.inbox_id(), bo.inbox_id());
    caro.create_group(None, None)?
        .send_message(b"sequence gap", SendMessageOpts::default())
        .await?;
    alix_group
        .send_message(b"after the rejected head", SendMessageOpts::default())
        .await?;
    let envelopes = bo
        .context
        .api()
        .query_all(
            [(topic.clone(), before.received)].into(),
            bo.context.api().limits().max_query_limit as u32,
        )
        .await?;
    assert_eq!(envelopes.len(), 1);
    let target = Cursor(
        envelopes[0]
            .meta
            .as_ref()
            .unwrap()
            .cursor
            .as_ref()
            .unwrap()
            .sequence_id,
    );
    assert!(target.0 > before.received.0 + 2);
    let mut reused = envelopes[0].envelope.clone().unwrap();
    let mut tampered = envelopes[0].clone();
    tampered.meta.as_mut().unwrap().cursor = Some(wire::Cursor {
        sequence_id: before.received.0 + 2,
    });
    let Some(wire::client_envelope::Payload::GroupMessage(message)) =
        tampered.envelope.as_mut().unwrap().payload.as_mut()
    else {
        panic!("group payload");
    };
    *message.data.last_mut().unwrap() ^= 1;
    let mut invalid = envelopes[0].clone();
    invalid.meta.as_mut().unwrap().cursor = Some(wire::Cursor {
        sequence_id: before.received.0 + 1,
    });
    let Some(wire::client_envelope::Payload::GroupMessage(message)) =
        invalid.envelope.as_mut().unwrap().payload.as_mut()
    else {
        panic!("group payload");
    };
    message.data = vec![0, 1, 0];
    let mut controller = controller(bo.context.clone());
    add_scope(&mut controller, 1, &topic);
    controller.reconcile()?;
    controller.registered([(topic.clone(), target)].into());
    controller.incoming(Some(Ok(IncomingEvent::OrderedBatch(
        OrderedEnvelopeBatch {
            topic: topic.clone(),
            after: before.received,
            envelopes: [invalid, tampered]
                .into_iter()
                .chain(envelopes)
                .map(Into::into)
                .collect(),
        },
    ))));
    assert!(controller.process_ready());
    assert_eq!(
        bo.context.db().topic_progress(&key)?.processed,
        Cursor(before.received.0 + 1)
    );
    let authenticator = bo_group.epoch_authenticator().await?;
    let GroupHeadOutcome::Progress {
        cursor,
        result: Err(error),
    } = bo_group.process_pending_group_head(None)?
    else {
        panic!("tampered ciphertext must be rejected before its valid generation is consumed");
    };
    assert_eq!(cursor, Cursor(before.received.0 + 2));
    let inner = match &error {
        GroupMessageProcessingError::OpenMlsProcessMessage(error)
        | GroupMessageProcessingError::OpenMlsProcessMessageWithAppData(
            ProcessMessageWithAppDataError::OpenMls(error),
        ) => error,
        other => panic!("expected an OpenMLS decryption error, got {other:?}"),
    };
    assert!(matches!(
        inner,
        ProcessMessageError::ValidationError(ValidationError::UnableToDecrypt(_))
    ));
    assert!(!matches!(
        inner,
        ProcessMessageError::ValidationError(ValidationError::UnableToDecrypt(
            MessageDecryptionError::SecretTreeError(SecretTreeError::SecretReuseError)
        ))
    ));
    assert_eq!(bo_group.epoch_authenticator().await?, authenticator);
    assert!(controller.process_ready());
    controller.refresh_statuses();
    assert_eq!(
        controller.state.statuses.lock()[&1].processing,
        IncomingProcessing::Complete
    );
    assert_eq!(bo.context.db().topic_progress(&key)?.processed, target);
    assert_eq!(
        bo_group.find_messages(&MsgQueryArgs::default())?.len(),
        message_count + 1
    );
    assert_eq!(
        bo_group
            .find_messages(&MsgQueryArgs::default())?
            .iter()
            .filter(|message| message.decrypted_message_bytes == b"after the rejected head")
            .count(),
        1
    );

    // Publish real ciphertext after its generation was consumed. Keep its TLS
    // shape, and sign the changed bytes so backend admission accepts them.
    let Some(wire::client_envelope::Payload::GroupMessage(message)) = &mut reused.payload else {
        panic!("group payload");
    };
    *message.data.last_mut().unwrap() ^= 1;
    alix_group.sign_as_member(message);
    let receipts = alix
        .context
        .api()
        .publish_units(vec![PublishUnit::single(reused)?])
        .await?;
    let (_, reused_cursor, _) = xmtp_api_backend::envelope::metadata(&receipts[0], topic.kind())?;
    let epoch_before_rejection = bo_group.epoch_authenticator().await?;
    alix_group
        .update_group_name("after the reused generation".into())
        .await?;
    let rows = bo
        .context
        .api()
        .query_all(
            [(topic.clone(), target)].into(),
            bo.context.api().limits().max_query_limit as u32,
        )
        .await?;
    // The reused ciphertext is followed by the AppDataUpdate proposal and
    // commit for the dictionary-native metadata update.
    assert_eq!(rows.len(), 3);
    let (_, commit_cursor, _) = xmtp_api_backend::envelope::metadata(
        rows.last().unwrap().meta.as_ref().unwrap(),
        topic.kind(),
    )?;
    controller.registered([(topic.clone(), commit_cursor)].into());
    controller.admit_received_batch(OrderedEnvelopeBatch {
        topic: topic.clone(),
        after: target,
        envelopes: rows.into_iter().map(Into::into).collect(),
    })?;
    let outcome = bo_group.process_pending_group_head(None)?;
    let GroupHeadOutcome::Progress {
        cursor,
        result: Err(error),
    } = outcome
    else {
        panic!("reused generation must reach terminal rejection");
    };
    assert_eq!(cursor, reused_cursor);
    let inner = match &error {
        GroupMessageProcessingError::OpenMlsProcessMessage(error)
        | GroupMessageProcessingError::OpenMlsProcessMessageWithAppData(
            ProcessMessageWithAppDataError::OpenMls(error),
        ) => error,
        other => panic!("expected an OpenMLS decryption error, got {other:?}"),
    };
    assert!(
        matches!(
            inner,
            ProcessMessageError::ValidationError(ValidationError::UnableToDecrypt(
                MessageDecryptionError::SecretTreeError(SecretTreeError::SecretReuseError)
            ))
        ),
        "expected a consumed generation, got {inner:?}"
    );
    assert_eq!(
        bo_group.epoch_authenticator().await?,
        epoch_before_rejection
    );
    assert_eq!(
        bo.context.db().topic_progress(&key)?.processed,
        reused_cursor
    );
    assert_eq!(
        bo.context
            .db()
            .read_last_rejection(&key)?
            .unwrap()
            .sequence_id,
        reused_cursor
    );
    assert!(controller.process_ready());
    // The dictionary-native metadata update has a proposal before its commit.
    assert!(controller.process_ready());
    assert_eq!(
        bo.context.db().topic_progress(&key)?.processed,
        commit_cursor
    );
    assert_eq!(bo_group.group_name()?, "after the reused generation");
    assert_eq!(
        bo_group.epoch_authenticator().await?,
        alix_group.epoch_authenticator().await?
    );
    alix_group
        .send_message(b"after the reused generation", SendMessageOpts::default())
        .await?;
    let sent = alix.context.db().topic_progress(&key)?.processed;
    crate::subscriptions::barrier::wait_through(&bo.context, [(topic, sent)].into(), None).await?;
    assert_eq!(
        bo_group
            .find_messages(&MsgQueryArgs::default())?
            .iter()
            .filter(|message| message.decrypted_message_bytes == b"after the reused generation")
            .count(),
        1
    );
}
