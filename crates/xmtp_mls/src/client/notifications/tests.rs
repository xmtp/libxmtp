use super::*;
use crate::{
    tester,
    worker::notifications::{
        self,
        tests::{config, support},
    },
};

#[xmtp_common::test(unwrap_try = true)]
fn notification_error_debug_does_not_expose_backend_text() {
    let secret = "recipient secret echoed by a backend";
    let error = NotificationError::from(xmtp_api::dyn_err(
        xmtp_proto::api::ApiClientError::OtherUnretryable(Box::new(tonic::Status::unknown(secret))),
    ));
    assert!(!format!("{error:?}").contains(secret));
    assert!(!error.to_string().contains(secret));
    assert!(!format!("{:?}", NotificationState::Failed(error)).contains(secret));
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_enable_without_task_runner_stores_nothing() {
    tester!(alix, disable_workers);
    let result = alix.enable_notifications(config()).await;
    assert!(matches!(result, Err(NotificationError::TaskRunnerDisabled)));
    let record = alix.db().notification_record()?;
    assert_eq!(record.push_state, 0);
    assert!(record.push_config.is_none());
    assert!(record.push_recipient_id.is_none());
    assert!(record.push_recipient_secret.is_none());
    assert_eq!(record.push_generation, 0);
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_apns_and_fcm_preserve_registration_rules_and_state() {
    use xmtp_db::ConnectionExt;
    use xmtp_proto::{backend_v1::register_request::Delivery, types::Topic};

    for channel in [
        NotificationChannel::Apns {
            token: "apns-exact-token".into(),
        },
        NotificationChannel::Fcm {
            token: "fcm-exact-token".into(),
        },
    ] {
        let (client, peer) = support::client().await;
        let group = client.create_group(None, None)?;
        group.update_consent_state(ConsentState::Denied)?;
        let excluded = client.create_group(None, None)?;
        let sync = MlsGroup::create_and_insert(
            client.context.clone(),
            xmtp_proto::types::ConversationType::Sync,
            Default::default(),
            Default::default(),
            None,
        )?;
        let config = NotificationConfig {
            channel,
            consent_states: vec![ConsentState::Unknown, ConsentState::Denied],
            include_welcomes: false,
            include_sync_groups: true,
            include_commits: true,
            metadata: vec![1, 2, 3, 4],
        };
        assert!(matches!(
            client.enable_notifications(config.clone()).await?,
            NotificationState::Enabled
        ));
        {
            let server = peer.state.lock();
            match (&config.channel, &server.delivery) {
                (NotificationChannel::Apns { token }, Some(Delivery::Apns(delivery))) => {
                    assert_eq!(&delivery.token, token)
                }
                (NotificationChannel::Fcm { token }, Some(Delivery::Fcm(delivery))) => {
                    assert_eq!(&delivery.token, token)
                }
                _ => panic!("registration must preserve the configured channel"),
            }
            assert_eq!(server.metadata, config.metadata);
        }
        notifications::run(&client.context).await?;
        {
            let server = peer.state.lock();
            assert_eq!(server.subscriptions.len(), 2);
            for id in [group.group_id, sync.group_id] {
                assert!(
                    server.subscriptions[&Topic::new_group_message(id).cloned_vec()]
                        .include_commits
                );
            }
            assert!(
                !server
                    .subscriptions
                    .contains_key(&Topic::new_group_message(excluded.group_id).cloned_vec())
            );
            assert!(!server.subscriptions.contains_key(
                &Topic::new_welcome_message(client.context.installation_id()).cloned_vec()
            ));
        }
        let record = client.db().notification_record()?;
        let snapshot = std::sync::Arc::new(
            client
                .db()
                .raw_query(|conn| Ok(conn.serialize_database_to_buffer().to_vec()))?,
        );
        tester!(restored, snapshot: snapshot, disable_workers);
        let persisted = restored.db().notification_record()?;
        assert_eq!(persisted.push_config, Some(encode(&config)?));
        assert_eq!(persisted.push_recipient_id, record.push_recipient_id);
        assert_eq!(
            persisted.push_recipient_secret,
            record.push_recipient_secret
        );
        assert_eq!(persisted.push_last_state, record.push_last_state);
        assert!(persisted.push_last_state.is_some());
        assert!(matches!(
            restored.notification_state()?,
            NotificationState::Enabled
        ));
        assert_eq!(restored.db().uploaded_topics()?.len(), 2);
        assert!(restored.group(&group.group_id)?.notifications_enabled()?);
        assert!(
            !restored
                .group(&excluded.group_id)?
                .notifications_enabled()?
        );
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_registration_capacity_error_uses_backoff() {
    let (client, peer) = support::client().await;
    peer.state.lock().next_error = Some(tonic::Code::ResourceExhausted);
    assert!(matches!(
        client.enable_notifications(config()).await,
        Err(NotificationError::ResourceExhausted)
    ));
    peer.state.lock().next_error = Some(tonic::Code::ResourceExhausted);
    assert!(matches!(
        notifications::run(&client.context).await,
        Err(NotificationError::ResourceExhausted)
    ));
    assert!(matches!(
        client.notification_state()?,
        NotificationState::Enabled
    ));
    assert!(client.db().notification_record()?.push_suppressed.is_none());
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_disable_keeps_identity_and_conversation_override() {
    let (client, peer) = support::client().await;
    let group = client.create_group(None, None)?;
    group.set_notifications(NotificationOverride::Disabled)?;
    assert!(matches!(
        client.enable_notifications(config()).await?,
        NotificationState::Enabled
    ));
    let first = client.db().notification_record()?;
    notifications::run(&client.context).await?;
    assert!(!client.db().uploaded_topics()?.is_empty());
    client.disable_notifications().await?;
    assert!(matches!(
        client.notification_state()?,
        NotificationState::Disabled
    ));
    assert!(client.db().uploaded_topics()?.is_empty());
    assert!(client.db().notification_record()?.push_config.is_none());
    let calls = peer.calls(support::Call::Register);
    assert_eq!(
        notifications::run(&client.context).await?,
        crate::worker::tasks::TaskOutcome::Done
    );
    assert_eq!(calls, peer.calls(support::Call::Register));
    client.enable_notifications(config()).await?;
    let second = client.db().notification_record()?;
    assert_eq!(first.push_recipient_id, second.push_recipient_id);
    assert_eq!(first.push_recipient_secret, second.push_recipient_secret);
    assert!(!group.notifications_enabled()?);
    peer.state.lock().registered = false;
    client.disable_notifications().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_failed_unregister_stays_disabled_without_background_retry() {
    for code in [tonic::Code::Unavailable, tonic::Code::DeadlineExceeded] {
        let (client, peer) = support::client().await;
        client.enable_notifications(config()).await?;
        notifications::run(&client.context).await?;
        let original = client.db().notification_record()?;
        peer.state.lock().next_error = Some(code);
        assert!(
            client
                .disable_notifications()
                .await
                .unwrap_err()
                .is_retryable()
        );
        assert!(matches!(
            client.notification_state()?,
            NotificationState::Disabled
        ));
        let disabled = client.db().notification_record()?;
        assert_eq!(disabled.push_recipient_id, original.push_recipient_id);
        assert_eq!(
            disabled.push_recipient_secret,
            original.push_recipient_secret
        );
        assert!(disabled.push_config.is_none());
        assert!(disabled.push_deadlines.is_none());
        assert!(client.db().uploaded_topics()?.is_empty());
        // PUSH-081 forbids calls from later tasks while disabled. The backend
        // retains the recipient only until expiry unless the caller retries.
        notifications::wake(&client.context)?;
        assert_eq!(
            notifications::run(&client.context).await?,
            crate::worker::tasks::TaskOutcome::Done
        );
        assert_eq!(peer.calls(support::Call::Unregister), 1);
        assert!(peer.state.lock().registered);
        client.disable_notifications().await?;
        assert_eq!(peer.calls(support::Call::Unregister), 2);
        assert!(!peer.state.lock().registered);
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_terminal_status_is_typed_and_stops_tasks() {
    let (client, peer) = support::client().await;
    for (code, expected) in [
        (
            tonic::Code::PermissionDenied,
            "NotificationError::PermissionDenied",
        ),
        (
            tonic::Code::InvalidArgument,
            "NotificationError::InvalidArgument",
        ),
        (tonic::Code::OutOfRange, "NotificationError::OutOfRange"),
        (
            tonic::Code::Unimplemented,
            "NotificationError::Unimplemented",
        ),
        (
            tonic::Code::FailedPrecondition,
            "NotificationError::ChannelNotConfigured",
        ),
    ] {
        peer.state.lock().next_error = Some(code);
        let error = client.enable_notifications(config()).await.unwrap_err();
        assert_eq!(error.error_code(), expected);
        let NotificationState::Failed(error) = client.notification_state()? else {
            panic!("terminal failure must persist");
        };
        assert_eq!(error.error_code(), expected);
        assert!(!error.is_retryable());
        let calls = peer.calls(support::Call::Register);
        assert_eq!(
            notifications::run(&client.context).await?,
            crate::worker::tasks::TaskOutcome::Done
        );
        assert_eq!(calls, peer.calls(support::Call::Register));
    }
    peer.state.lock().next_error = Some(tonic::Code::Unavailable);
    assert!(
        client
            .enable_notifications(config())
            .await
            .unwrap_err()
            .is_retryable()
    );
    assert!(matches!(
        client.notification_state()?,
        NotificationState::Enabled
    ));
    notifications::run(&client.context).await?;
    assert!(client.db().notification_record()?.push_last_state.is_some());
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_effective_rule_and_duplicate_dms() {
    let (client, _) = support::client().await;
    tester!(bo, disable_workers);
    let group = client.create_group(None, None)?;
    let dm1 = MlsGroup::create_dm_and_insert(
        &client.context,
        xmtp_db::group::GroupMembershipState::Allowed,
        bo.inbox_id().to_string(),
        Default::default(),
        None,
    )?;
    let dm2 = MlsGroup::create_dm_and_insert(
        &client.context,
        xmtp_db::group::GroupMembershipState::Allowed,
        bo.inbox_id().to_string(),
        Default::default(),
        None,
    )?;
    assert_ne!(dm1.group_id, dm2.group_id);
    client.enable_notifications(config()).await?;
    assert!(group.notifications_enabled()?);
    group.update_consent_state(ConsentState::Denied)?;
    assert!(!group.notifications_enabled()?);
    group.set_notifications(NotificationOverride::Enabled)?;
    assert!(group.notifications_enabled()?);
    group.set_notifications(NotificationOverride::Default)?;
    assert!(!group.notifications_enabled()?);
    notifications::run(&client.context).await?;
    let topics = client.db().uploaded_topics()?;
    for group in [&dm1, &dm2] {
        let topic = xmtp_proto::types::Topic::new_group_message(group.group_id).cloned_vec();
        assert!(topics.iter().any(|row| row.topic == topic));
    }
}

xmtp_common::if_native! {
    #[xmtp_common::test(unwrap_try = true)]
    async fn notification_real_backend_registers_and_restores_on_start() {
        let backend = crate::utils::backend::EphemeralBackend::start("[push.http]\nallow_private_addresses = true").await?;
        tester!(alix, backend: &backend);
        alix.client.workers.shutdown().await;
        alix.create_group(None, None)?;
        alix.enable_notifications(config()).await?;
        let original = alix.db().notification_record()?;
        assert!(original.push_last_state.is_some());
        // No upload happened before the snapshot. Start-up must compute it from groups.
        assert!(alix.db().uploaded_topics()?.is_empty());
        let snapshot = std::sync::Arc::new(alix.db_snapshot());
        tester!(restored, snapshot: snapshot, backend: &backend);
        xmtp_common::wait_for_eq(|| async { restored.db().uploaded_topics().unwrap().len() }, 2).await?;
        assert_eq!(original.push_recipient_id, restored.db().notification_record()?.push_recipient_id);
        restored.disable_notifications().await?;
    }
}
