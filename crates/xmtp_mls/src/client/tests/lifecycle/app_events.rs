//! Client lifecycle tests: app events.

use super::*;

// verifies: EVENT-001, EVENT-010, EVENT-024
#[xmtp_common::test(unwrap_try = true)]
async fn registration_event_waits_for_visibility_and_fires_once() {
    use crate::utils::DefaultTestClientCreator;
    use crate::utils::test::{identity_setup, register_client};
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_events::{EventFilter, EventKind};
    use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;
    use xmtp_proto::api_client::{ApiBuilder, XmtpTestClient};

    let wallet = generate_local_wallet();
    let client = Client::builder(identity_setup(wallet.clone()))
        .temp_store()
        .await
        .api_client(DefaultTestClientCreator::create().build()?)
        .default_mls_store()?
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let events = client
        .context
        .events()
        .subscribe(EventFilter::new([EventKind::IdentityRegistered]), Some(4));
    register_client(&client, wallet).await;
    assert!(matches!(
        events.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::IdentityRegistered(registered)), ..
        }] if registered.inbox_id == client.inbox_id()
            && registered.installation_key == client.installation_id.to_vec()
    ));
    client
        .wait_for_registration_visible(Default::default())
        .await?;
    client
        .wait_for_registration_visible(Default::default())
        .await?;
    assert!(events.drain().is_empty());
}

// verifies: EVENT-054
#[xmtp_common::test(unwrap_try = true)]
async fn dropping_the_last_client_handle_closes_app_events() {
    tester!(alix, disable_workers);
    let bus = alix.context.events().clone();
    let subscription = bus.subscribe_app(xmtp_events::EventFilter::new([
        xmtp_events::EventKind::ClientLockoutChanged,
    ]))?;
    drop(alix);
    assert!(subscription.is_closed());
    assert!(subscription.drain().is_empty());
}

// verifies: EVENT-001, EVENT-005, EVENT-010, EVENT-023
#[xmtp_common::test(unwrap_try = true)]
async fn created_and_welcomed_groups_emit_one_join_event() {
    tester!(alix, disable_workers);
    tester!(caro, disable_workers);
    tester!(bo, disable_workers);
    let created = alix
        .context
        .events()
        .subscribe_app(xmtp_events::EventFilter::new([
            xmtp_events::EventKind::ConversationJoined,
        ]))?;
    let welcomed = bo
        .context
        .events()
        .subscribe_app(xmtp_events::EventFilter::new([
            xmtp_events::EventKind::ConversationJoined,
        ]))?;
    let group = alix.create_group(None, None)?;
    assert!(matches!(
        created.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::ConversationJoined(joined)), ..
        }] if joined.group_id == group.group_id.to_vec()
            && joined.conversation_type == xmtp_events::ConversationType::Group
            && joined.origin == xmtp_events::JoinOrigin::Created
            && joined.adder_inbox_id.is_none()
    ));
    group.add_members(&[caro.inbox_id()]).await?;
    let caro_group = caro.sync_welcomes().await?.pop()?;
    group
        .update_admin_list(
            crate::groups::UpdateAdminListType::Add,
            caro.inbox_id().to_string(),
        )
        .await?;
    caro_group.sync().await?;
    caro_group.add_members(&[bo.inbox_id()]).await?;
    let bo_group = bo.sync_welcomes().await?.pop()?;
    assert_eq!(bo_group.group_id, group.group_id);
    assert!(matches!(
        welcomed.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::ConversationJoined(joined)), ..
        }] if joined.group_id == group.group_id.to_vec()
            && joined.conversation_type == xmtp_events::ConversationType::Group
            && joined.origin == xmtp_events::JoinOrigin::Welcomed
            && joined.adder_inbox_id.as_deref() == Some(caro.inbox_id())
    ));
    bo.sync_welcomes().await?;
    assert!(welcomed.drain().is_empty());
    assert!(created.drain().is_empty());
}

// verifies: EVENT-001, EVENT-005
#[xmtp_common::test(unwrap_try = true)]
async fn created_and_welcomed_dms_emit_one_join_event() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let created = alix
        .context
        .events()
        .subscribe_app(xmtp_events::EventFilter::new([
            xmtp_events::EventKind::ConversationJoined,
        ]))?;
    let welcomed = bo
        .context
        .events()
        .subscribe_app(xmtp_events::EventFilter::new([
            xmtp_events::EventKind::ConversationJoined,
        ]))?;

    let dm = alix
        .create_dm_by_inbox_id(bo.inbox_id().to_string(), None)
        .await?;
    assert!(matches!(
        created.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::ConversationJoined(joined)), context, ..
        }] if joined.group_id == dm.group_id.to_vec()
            && joined.conversation_type == xmtp_events::ConversationType::Dm
            && joined.origin == xmtp_events::JoinOrigin::Created
            && joined.adder_inbox_id.is_none()
            && context.dm_identifier == Some(dm.dm_id.clone().expect("DM ID").into_bytes())
    ));

    let bo_dm = bo.sync_welcomes().await?.pop()?;
    assert_eq!(bo_dm.group_id, dm.group_id);
    assert!(matches!(
        welcomed.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::ConversationJoined(joined)), ..
        }] if joined.group_id == dm.group_id.to_vec()
            && joined.conversation_type == xmtp_events::ConversationType::Dm
            && joined.origin == xmtp_events::JoinOrigin::Welcomed
    ));
    bo.sync_welcomes().await?;
    assert!(welcomed.drain().is_empty());
    assert!(created.drain().is_empty());
}

// verifies: EVENT-007, EVENT-010
#[xmtp_common::test(unwrap_try = true)]
async fn local_delete_emits_once_after_the_row_is_deleted() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let message_id = group.prepare_message_for_later_publish(b"local delete", false, None)?;
    let events = alix.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::MessageDeleted]),
        Some(10),
    );
    assert_eq!(alix.delete_message(message_id.clone())?, 1);
    assert!(alix.context.db().get_group_message(&message_id)?.is_none());
    assert_eq!(alix.delete_message(message_id.clone())?, 0);
    assert!(matches!(
        events.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::MessageDeleted(deleted)), ..
        }] if deleted.group_id == group.group_id.to_vec()
            && deleted.message_id == message_id
            && deleted.cause == xmtp_events::DeletionCause::DeletedLocally
    ));
}

// verifies: EVENT-010
#[xmtp_common::test(unwrap_try = true)]
async fn one_consent_batch_emits_only_the_final_state_for_an_entity() {
    tester!(alix, disable_workers);
    let events = alix.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::ConsentChanged]),
        Some(10),
    );
    let act = |state| {
        crate::client::ConsentAct::new(ConsentType::InboxId, state, "duplicate-consent".into())
    };
    alix.set_consent_states(&[act(ConsentState::Denied), act(ConsentState::Allowed)])
        .await?;
    assert_eq!(
        alix.context
            .db()
            .consent_row(&xmtp_db::consent_record::ConsentKey::inbox(
                "duplicate-consent"
            ))?
            .map(|record| record.state),
        Some(ConsentState::Allowed)
    );
    assert!(matches!(
        events.drain().as_slice(),
        [xmtp_events::EventEnvelope {
            client: Some(xmtp_events::ClientEvent::ConsentChanged(change)), ..
        }] if change.entity == "duplicate-consent"
            && change.state == xmtp_events::ConsentState::Allowed
    ));
}

// verifies: EVENT-001, EVENT-059, CONS-055
#[xmtp_common::test(unwrap_try = true)]
async fn app_consent_acts_emit_once_per_change_and_store_every_repeat() {
    use crate::subscriptions::internal::InternalEvent;
    use crate::worker::device_sync::preference_sync::PreferenceUpdate;
    use xmtp_db::consent_record::{ConsentKey, ConsentState::*, ConsentType::*};
    use xmtp_events::{ConsentEntityKind::*, ConsentState as Event, EventEnvelope};
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let id = hex::encode(group.group_id);
    let id = id.as_str();
    let events = alix
        .context
        .events()
        .subscribe_app(xmtp_events::EventFilter::new([
            xmtp_events::EventKind::ConsentChanged,
        ]))?;
    let preferences = alix.context.events().subscribe(
        xmtp_events::EventFilter::default()
            .with_internal(|event| matches!(event, InternalEvent::PreferencesChanged { .. })),
        Some(16),
    );

    for (entity_type, entity, state, kind, expected) in [
        (InboxId, "inbox", Allowed, Inbox, Event::Allowed),
        (ConversationId, id, Denied, Conversation, Event::Denied),
        (ConversationId, id, Unknown, Conversation, Event::Unknown),
    ] {
        let key = ConsentKey {
            entity_type,
            entity: entity.into(),
        };
        let act = crate::client::ConsentAct::new(entity_type, state, entity.into());
        alix.set_consent_states(std::slice::from_ref(&act)).await?;
        assert!(matches!(
            events.drain().as_slice(),
            [EventEnvelope { client: Some(xmtp_events::ClientEvent::ConsentChanged(change)), .. }]
                if change.entity_kind == kind && change.entity == entity && change.state == expected
        ));
        preferences.drain();
        let first = alix.context.db().consent_row(&key)?.unwrap();

        // A same-state repeat emits no app event; it moves the time
        // and is owed again.
        alix.set_consent_states(std::slice::from_ref(&act)).await?;
        assert!(events.drain().is_empty());
        let stored = alix.context.db().consent_row(&key)?.unwrap();
        assert_eq!(stored.state, state);
        assert!(stored.consented_at_ns > first.consented_at_ns);
        assert!(stored.owed_seq > first.owed_seq);
        assert!(matches!(
            preferences.drain().as_slice(),
            [EventEnvelope { internal: Some(InternalEvent::PreferencesChanged { updates }), .. }]
                if matches!(updates.as_slice(), [PreferenceUpdate::Consent(published)]
                if published.consented_at_ns == stored.consented_at_ns)
        ));
    }
}

// verifies: CONS-041
#[xmtp_common::test(unwrap_try = true)]
async fn should_stream_consent() {
    let alix = Tester::builder().sync_worker().build().await;
    let bo = Tester::new().await;

    let receiver = alix.context.events().subscribe(
        xmtp_events::EventFilter::default().with_internal(|event| {
            matches!(
                event,
                crate::subscriptions::internal::InternalEvent::PreferencesChanged { .. }
            )
        }),
        Some(1024),
    );
    let stream = receiver.stream_consent_updates();
    futures::pin_mut!(stream);

    let group = alix
        .create_group_with_members(&[bo.inbox_id().to_string()], None, None)
        .await
        .unwrap();
    xmtp_common::time::sleep(std::time::Duration::from_millis(500)).await;

    // first record is denied consent to the group.
    group.update_consent_state(ConsentState::Denied).unwrap();

    xmtp_common::time::sleep(std::time::Duration::from_millis(500)).await;

    // second is allowing consent for the group
    alix.set_consent_states(&[crate::client::ConsentAct::new(
        ConsentType::ConversationId,
        ConsentState::Allowed,
        hex::encode(group.group_id),
    )])
    .await
    .unwrap();

    xmtp_common::time::sleep(std::time::Duration::from_millis(500)).await;

    // third allowing consent for bo inbox id
    alix.set_consent_states(&[crate::client::ConsentAct::new(
        ConsentType::InboxId,
        ConsentState::Allowed,
        bo.inbox_id().to_string(),
    )])
    .await
    .unwrap();

    // First consent update from creating the group
    let item = stream.next().await??;
    assert_eq!(item.len(), 1);
    assert_eq!(item[0].entity_type, ConsentType::ConversationId);
    assert_eq!(item[0].entity, hex::encode(group.group_id));
    assert_eq!(item[0].state, ConsentState::Allowed);

    let item = stream.next().await??;
    assert_eq!(item.len(), 1);
    assert_eq!(item[0].entity_type, ConsentType::ConversationId);
    assert_eq!(item[0].entity, hex::encode(group.group_id));
    assert_eq!(item[0].state, ConsentState::Denied);

    let item = stream.next().await??;
    assert_eq!(item.len(), 1);
    assert_eq!(item[0].entity_type, ConsentType::ConversationId);
    assert_eq!(item[0].entity, hex::encode(group.group_id));
    assert_eq!(item[0].state, ConsentState::Allowed);

    let item = stream.next().await??;
    assert_eq!(item.len(), 1);
    assert_eq!(item[0].entity_type, ConsentType::InboxId);
    assert_eq!(item[0].entity, bo.inbox_id());
    assert_eq!(item[0].state, ConsentState::Allowed);
}
