use super::*;
use crate::{
    tester,
    worker::notifications::{
        self,
        tests::{config, support},
    },
};
use xmtp_db::consent_record::ConsentState;

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

// verifies: PUSH-266
#[xmtp_common::test(unwrap_try = true)]
async fn notification_http_signing_key_boundaries_before_persistence() {
    for size in [15, 16, 64, 65] {
        let (client, peer) = support::client().await;
        let config = NotificationConfig::new(NotificationChannel::Http {
            url: "https://example.test".into(),
            signing_key: vec![7; size].into(),
        });
        let result = client.enable_notifications(config).await;
        let record = client.db().notification_record()?;
        if (16..=64).contains(&size) {
            assert!(matches!(result?, NotificationState::Enabled));
            assert_eq!(peer.calls(support::Call::Register), 1);
            assert!(record.push_config.is_some());
        } else {
            assert!(matches!(result, Err(NotificationError::InvalidArgument)));
            assert_eq!(peer.calls(support::Call::Register), 0);
            assert!(record.push_config.is_none());
            assert!(record.push_recipient_id.is_none());
            assert!(record.push_recipient_secret.is_none());
            assert_eq!(record.push_generation, 0);
            assert!(matches!(
                client.notification_state()?,
                NotificationState::Disabled
            ));
        }
    }
}

// verifies: PUSH-266
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
        assert_eq!(persisted.push_config, Some(encode_config(&config)?));
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

// verifies: PUSH-266
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
    assert!(!client.db().notification_record()?.push_unregister_owed);
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
        assert!(disabled.push_unregister_owed);
        assert!(client.db().uploaded_topics()?.is_empty());
        // Later tasks must not make calls while disabled. The backend
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
        assert!(!client.db().notification_record()?.push_unregister_owed);
        assert!(!peer.state.lock().registered);
    }
}

// verifies: PUSH-265, PUSH-266
#[xmtp_common::test(unwrap_try = true)]
async fn notification_failed_unregister_restart_resets_before_reregistering() {
    use xmtp_db::ConnectionExt;
    use xmtp_proto::{backend_v1::register_request::Delivery, types::Topic};

    let (client, peer) = support::client().await;
    let group = client.create_group(None, None)?;
    let group_id = group.group_id;
    let topic = Topic::new_group_message(group_id).cloned_vec();
    let mut initial = config();
    initial.include_welcomes = false;
    client.enable_notifications(initial).await?;
    notifications::run(&client.context).await?;
    assert!(peer.state.lock().subscriptions.contains_key(&topic));
    assert_eq!(client.db().uploaded_topics()?.len(), 1);
    let original = client.db().notification_record()?;

    peer.state.lock().next_error = Some(tonic::Code::Unavailable);
    assert!(
        client
            .disable_notifications()
            .await
            .unwrap_err()
            .is_retryable()
    );
    assert!(client.db().uploaded_topics()?.is_empty());
    assert!(peer.state.lock().subscriptions.contains_key(&topic));
    assert!(client.db().notification_record()?.push_unregister_owed);

    let snapshot = std::sync::Arc::new(
        client
            .db()
            .raw_query(|conn| Ok(conn.serialize_database_to_buffer().to_vec()))?,
    );
    drop(group);
    drop(client);
    let restored = support::client_from_snapshot(snapshot, peer.clone()).await;
    assert!(restored.db().uploaded_topics()?.is_empty());
    assert!(restored.db().notification_record()?.push_unregister_owed);

    let mut replacement = config();
    replacement.consent_states.clear();
    replacement.channel = NotificationChannel::Http {
        url: "https://new.example.test/push".into(),
        signing_key: vec![9; 32].into(),
    };
    let before_enable = peer.state.lock().calls.len();
    restored.enable_notifications(replacement).await?;
    assert_eq!(
        &peer.state.lock().calls[before_enable..],
        &[support::Call::Unregister, support::Call::Register]
    );
    assert!(peer.state.lock().subscriptions.is_empty());
    let after = restored.db().notification_record()?;
    assert_eq!(after.push_recipient_id, original.push_recipient_id);
    assert_eq!(after.push_recipient_secret, original.push_recipient_secret);
    assert!(!restored.group(&group_id)?.notifications_enabled()?);
    assert!(matches!(
        restored.notification_state()?,
        NotificationState::Enabled
    ));
    assert_eq!(peer.response().topic_count, 0);
    assert!(!after.push_unregister_owed);
    assert!(matches!(
        peer.state.lock().delivery,
        Some(Delivery::Http(ref delivery)) if delivery.url == "https://new.example.test/push"
    ));

    notifications::run(&restored.context).await?;
    notifications::run(&restored.context).await?;
    let mut record = restored.db().notification_record()?;
    record.push_deadlines = Some(encode(&notifications::Deadlines::default())?);
    restored.db().save_notification_record(&record)?;
    notifications::run(&restored.context).await?;
    assert_eq!(peer.calls(support::Call::Register), 3);
    let welcome = Topic::new_welcome_message(restored.context.installation_id()).cloned_vec();
    assert_eq!(
        peer.state
            .lock()
            .subscriptions
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec![welcome.clone()]
    );
    let uploaded = restored.db().uploaded_topics()?;
    assert_eq!(uploaded.len(), 1);
    assert_eq!(uploaded[0].topic, welcome);
    assert!(!restored.db().notification_record()?.push_repairing);
    assert_eq!(peer.calls(support::Call::Update), 2);
    assert_eq!(peer.calls(support::Call::Unregister), 2);
}

// verifies: PUSH-265
#[xmtp_common::test(unwrap_try = true)]
async fn notification_enable_reset_failure_stays_enabled_and_the_task_retries_it() {
    let (client, peer) = support::client().await;
    client.enable_notifications(config()).await?;
    notifications::run(&client.context).await?;
    assert!(!peer.state.lock().subscriptions.is_empty());
    peer.state.lock().next_error = Some(tonic::Code::Unavailable);
    assert!(
        client
            .disable_notifications()
            .await
            .unwrap_err()
            .is_retryable()
    );

    let before_enable = peer.state.lock().calls.len();
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
    assert_eq!(
        &peer.state.lock().calls[before_enable..],
        &[support::Call::Unregister]
    );
    assert!(client.db().notification_record()?.push_unregister_owed);

    notifications::run(&client.context).await?;
    assert_eq!(
        &peer.state.lock().calls[before_enable..],
        &[
            support::Call::Unregister,
            support::Call::Unregister,
            support::Call::Register,
        ]
    );
    assert!(!client.db().notification_record()?.push_unregister_owed);
    assert!(peer.state.lock().subscriptions.is_empty());
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

#[xmtp_common::test(unwrap_try = true)]
async fn notification_empty_tokens_and_url_reach_backend_failure() {
    for channel in [
        NotificationChannel::Apns {
            token: String::new(),
        },
        NotificationChannel::Fcm {
            token: String::new(),
        },
        NotificationChannel::Http {
            url: String::new(),
            signing_key: vec![7; 16].into(),
        },
    ] {
        let (client, peer) = support::client().await;
        peer.state.lock().next_error = Some(tonic::Code::InvalidArgument);
        assert!(matches!(
            client
                .enable_notifications(NotificationConfig::new(channel))
                .await,
            Err(NotificationError::InvalidArgument)
        ));
        let record = client.db().notification_record()?;
        assert_eq!(peer.calls(support::Call::Register), 1);
        assert!(record.push_config.is_some());
        assert!(record.push_recipient_id.is_some());
        assert!(record.push_recipient_secret.is_some());
        assert_eq!(record.push_generation, 1);
        assert!(matches!(
            client.notification_state()?,
            NotificationState::Failed(NotificationError::InvalidArgument)
        ));
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn notification_backend_invalid_argument_fails_after_registration() {
    let (client, peer) = support::client().await;
    peer.state.lock().next_error = Some(tonic::Code::InvalidArgument);
    assert!(matches!(
        client
            .enable_notifications(NotificationConfig::new(NotificationChannel::Http {
                url: "http://example.test".into(),
                signing_key: vec![7; 16].into(),
            }))
            .await,
        Err(NotificationError::InvalidArgument)
    ));
    let record = client.db().notification_record()?;
    assert_eq!(peer.calls(support::Call::Register), 1);
    assert!(record.push_config.is_some());
    assert!(record.push_recipient_id.is_some());
    assert!(record.push_recipient_secret.is_some());
    assert_eq!(record.push_generation, 1);
    assert!(matches!(
        client.notification_state()?,
        NotificationState::Failed(NotificationError::InvalidArgument)
    ));
}

/// The stored configuration is the JSON a plain `Vec<u8>` signing key
/// produced, recorded from the code before the key became a `Secret`.
#[xmtp_common::test(unwrap_try = true)]
async fn notification_config_column_keeps_its_json() {
    use xmtp_db::{ConnectionExt, diesel::prelude::*, schema::user_preferences::dsl};
    const STORED: &str = concat!(
        r#"{"channel":{"Http":{"url":"https://push.example.test/hook","signing_key":"#,
        "[192,193,194,195,196,197,198,199,200,201,202,203,204,205,206,207,",
        "208,209,210,211,212,213,214,215,216,217,218,219,220,221,222,223]}},",
        r#""consent_states":["Allowed","Unknown"],"include_welcomes":true,"#,
        r#""include_sync_groups":false,"include_commits":true}"#
    );
    let signing_key: Vec<u8> = (0xc0..0xe0).collect();
    let mut config = NotificationConfig::new(NotificationChannel::Http {
        url: "https://push.example.test/hook".into(),
        signing_key: signing_key.clone().into(),
    });
    config.consent_states = vec![ConsentState::Allowed, ConsentState::Unknown];
    config.include_commits = true;
    assert_eq!(encode(&config)?, STORED.as_bytes());

    let (client, _peer) = support::client().await;
    assert!(matches!(
        client.enable_notifications(config).await?,
        NotificationState::Enabled
    ));
    let raw: Option<Vec<u8>> = client
        .db()
        .raw_query(|conn| dsl::user_preferences.select(dsl::push_config).first(conn))?;
    assert_eq!(raw.as_deref(), Some(STORED.as_bytes()));
    let stored = stored_config(&client.db().notification_record()?)?;
    let NotificationChannel::Http {
        signing_key: key, ..
    } = stored.channel
    else {
        panic!("the stored channel is HTTP");
    };
    assert_eq!(key.as_slice(), signing_key.as_slice());
}
