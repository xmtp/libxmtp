//! Facade work that crosses into the notification runner: imports that change
//! what it must keep subscribed, the client's task runner and workers driving
//! notification sync, and the notification contracts of a client's build and
//! restart.

use crate::context::CoreContext;
use crate::utils::test::notifications;
use crate::worker::device_sync::archive::insert_elements_for_test;
use xmtp_db::consent_record::StoredConsentRecord;
use xmtp_db::prelude::*;
use xmtp_proto::xmtp::device_sync::{BackupElement, backup_element::Element};

/// An import that changes consent wakes the notification runner, which
/// removes the topic of the conversation the archive denies.
// verifies: PUSH-260
#[xmtp_common::test(unwrap_try = true)]
async fn archived_consent_wakes_notification_sync() {
    use crate::client::notifications::test_support::{config, task_hash};
    use crate::task_handlers::MlsTaskHandlers;
    use crate::worker::tasks::TaskWorker;
    use xmtp_common::{StreamHandle, time::Duration, time::timeout, wait_for_eq};
    let (client, peer) = notifications::client().await;
    let group = client.create_group(None, None)?;
    client.enable_notifications(config()).await?;
    let mut task_worker = TaskWorker::new(client.context.clone(), MlsTaskHandlers);
    let runner = xmtp_common::spawn(None, async move { task_worker.run().await });
    // Wait until the runner is idle, so only a wake runs it within the hour.
    let idle = || async {
        let tasks = client.db().get_tasks().unwrap();
        let task = tasks
            .iter()
            .find(|task| task.data_hash == task_hash().as_ref());
        let soon = xmtp_common::time::now_ns() + xmtp_common::NS_IN_MIN;
        task.is_some_and(|task| task.next_attempt_at_ns > soon)
    };
    timeout(Duration::from_secs(5), wait_for_eq(idle, true)).await??;
    let topic = xmtp_proto::types::Topic::new_group_message(group.group_id).cloned_vec();
    let holds = || async { peer.state.lock().subscriptions.contains_key(&topic) };
    assert!(holds().await);

    let revision = client.context.task_channels().notification_revision();
    let denied = StoredConsentRecord {
        entity_type: xmtp_db::consent_record::ConsentType::ConversationId,
        state: xmtp_db::consent_record::ConsentState::Denied,
        entity: hex::encode(group.group_id),
        consented_at_ns: xmtp_common::time::now_ns(),
        owed_seq: 0,
    };
    let mut elements = futures::stream::iter([Ok::<_, std::io::Error>(BackupElement {
        element: Some(Element::Consent(denied.into())),
    })]);
    insert_elements_for_test(&mut elements, &client.context).await?;
    assert!(client.context.task_channels().notification_revision() > revision);
    timeout(Duration::from_secs(5), wait_for_eq(holds, false)).await??;
    runner.abort_handle().end();
}

/// Notification sync driven by the client's task runner.
mod runner {
    use crate::client::notifications::test_support::{RETRY_NS, config, task_hash};
    use crate::client::notifications::{
        NotificationError, NotificationOverride, NotificationState,
    };
    #[cfg(not(target_arch = "wasm32"))]
    use crate::tester;
    use crate::utils::test::notifications::{self as support, Call};
    use crate::worker::notifications::{run, wake};
    use xmtp_common::{NS_IN_HOUR, RetryableError, time, time::Duration};
    #[cfg(not(target_arch = "wasm32"))]
    use xmtp_common::{NS_IN_SEC, StreamHandle, time::timeout};
    #[cfg(not(target_arch = "wasm32"))]
    use xmtp_db::consent_record::ConsentState;
    use xmtp_db::prelude::*;
    use xmtp_mls_core::worker::tasks::TaskOutcome;
    use xmtp_proto::types::Topic;

    #[xmtp_common::test(unwrap_try = true)]
    async fn notification_repair_reaches_2500_topics_in_three_turns() {
        use crate::task_handlers::MlsTaskHandlers;
        use xmtp_mls_core::worker::tasks::TaskWorker;
        let (client, peer) = support::client().await;
        // The welcome topic and 2,499 real MLS groups form the desired set.
        for _ in 0..2499 {
            client.create_group(None, None)?;
        }
        client.enable_notifications(config()).await?;
        peer.state.lock().extra = 1;
        wake(&client.context)?;
        let next_task = || {
            client
                .db()
                .get_tasks()
                .unwrap()
                .into_iter()
                .find(|task| task.data_hash == task_hash().as_ref())
                .unwrap()
        };
        xmtp_mls_core::worker::tasks::test_support::run_and_reschedule_task(
            &TaskWorker::new(client.context.clone(), MlsTaskHandlers),
            next_task(),
        )
        .await?;
        assert_eq!(
            client
                .db()
                .uploaded_topics()?
                .iter()
                .filter(|row| row.stale)
                .count(),
            1000
        );
        xmtp_mls_core::worker::tasks::test_support::run_and_reschedule_task(
            &TaskWorker::new(client.context.clone(), MlsTaskHandlers),
            next_task(),
        )
        .await?;
        xmtp_mls_core::worker::tasks::test_support::run_and_reschedule_task(
            &TaskWorker::new(client.context.clone(), MlsTaskHandlers),
            next_task(),
        )
        .await?;
        assert_eq!(peer.calls(Call::Update), 3);
        assert_eq!(peer.state.lock().subscriptions.len(), 2500);
        assert_eq!(client.db().uploaded_topics()?.len(), 2500);
        assert!(client.db().notification_record()?.push_repairing);
        assert_eq!(
            client
                .db()
                .uploaded_topics()?
                .iter()
                .filter(|row| row.stale)
                .count(),
            500
        );
        xmtp_mls_core::worker::tasks::test_support::run_and_reschedule_task(
            &TaskWorker::new(client.context.clone(), MlsTaskHandlers),
            next_task(),
        )
        .await?;
        assert!(!client.db().notification_record()?.push_repairing);
        assert!(client.db().uploaded_topics()?.iter().all(|row| !row.stale));
        assert_eq!(
            peer.state.lock().batches,
            vec![(1000, 0), (1000, 0), (1000, 0), (500, 0)]
        );
        xmtp_mls_core::worker::tasks::test_support::run_and_reschedule_task(
            &TaskWorker::new(client.context.clone(), MlsTaskHandlers),
            next_task(),
        )
        .await?;
        assert_eq!(peer.calls(Call::Update), 4);
        assert_eq!(next_task().attempts, 0);
        assert!(next_task().next_attempt_at_ns > time::now_ns());
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn notification_task_is_durable_coalesced_and_retries_after_sixty_seconds() {
        use crate::task_handlers::MlsTaskHandlers;
        use xmtp_mls_core::worker::tasks::TaskWorker;
        let (client, peer) = support::client().await;
        client.enable_notifications(config()).await?;
        for _ in 0..20 {
            wake(&client.context)?;
        }
        let tasks = client.db().get_tasks()?;
        let rows: Vec<_> = tasks
            .into_iter()
            .filter(|row| row.data_hash == task_hash().as_ref())
            .collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].expires_at_ns, i64::MAX);
        assert_eq!(rows[0].max_attempts, i32::MAX);
        let task = || {
            client
                .db()
                .get_tasks()
                .unwrap()
                .into_iter()
                .find(|row| row.data_hash == task_hash().as_ref())
                .unwrap()
        };
        peer.state.lock().next_error = Some(tonic::Code::Unavailable);
        let before = time::now_ns();
        xmtp_mls_core::worker::tasks::test_support::run_and_reschedule_task(
            &TaskWorker::new(client.context.clone(), MlsTaskHandlers),
            rows[0].clone(),
        )
        .await?;
        let row = task();
        assert_eq!(row.attempts, 1);
        assert!(row.next_attempt_at_ns >= before + RETRY_NS);
        assert!(row.next_attempt_at_ns <= time::now_ns() + RETRY_NS);
        let retry_at = row.next_attempt_at_ns;
        let calls = peer.calls(Call::Update);
        for _ in 0..20 {
            wake(&client.context)?;
        }
        let row = task();
        assert_eq!(row.next_attempt_at_ns, retry_at);
        xmtp_mls_core::worker::tasks::test_support::run_and_reschedule_task(
            &TaskWorker::new(client.context.clone(), MlsTaskHandlers),
            row.clone(),
        )
        .await?;
        assert_eq!(peer.calls(Call::Update), calls);
        // Make the stored retry due without waiting for wall-clock time.
        client
            .db()
            .update_task(row.id, row.attempts, before, before)?;
        let mut due = row;
        due.next_attempt_at_ns = before;
        xmtp_mls_core::worker::tasks::test_support::run_and_reschedule_task(
            &TaskWorker::new(client.context.clone(), MlsTaskHandlers),
            due,
        )
        .await?;
        let row = task();
        assert_eq!(row.attempts, 0);
        let calls = peer.calls(Call::Update);
        let before = time::now_ns();
        let TaskOutcome::RescheduleAt(next) = run(&client.context).await? else {
            panic!("enabled task must remain");
        };
        assert_eq!(peer.calls(Call::Update), calls);
        assert!(next >= before + NS_IN_HOUR);
        client.db().update_task(row.id, 0, before, next)?;
        wake(&client.context)?;
        let advanced = task();
        assert!(advanced.next_attempt_at_ns < next);
        // A missed hint is repaired by the same local scan when the task next runs.
        client.create_group(None, None)?;
        run(&client.context).await?;
        assert_eq!(peer.calls(Call::Update), calls + 1);

        // A presumed replacement can still exceed the cap when the remote lost it.
        let group = client.create_group(None, None)?;
        run(&client.context).await?;
        {
            let mut state = peer.state.lock();
            state
                .subscriptions
                .remove(&Topic::new_group_message(group.group_id).cloned_vec());
            state.limit = Some(state.subscriptions.len());
        }
        client.db().mark_uploaded_topics_stale()?;
        wake(&client.context)?;
        let before = time::now_ns();
        xmtp_mls_core::worker::tasks::test_support::run_and_reschedule_task(
            &TaskWorker::new(client.context.clone(), MlsTaskHandlers),
            task(),
        )
        .await?;
        let refused = task();
        assert_eq!(
            refused.attempts, 1,
            "refused repair must use durable backoff"
        );
        assert!(refused.next_attempt_at_ns >= before + RETRY_NS);
        let calls = peer.calls(Call::Update);
        group.set_notifications(NotificationOverride::Disabled)?;
        wake(&client.context)?;
        assert_eq!(task().next_attempt_at_ns, refused.next_attempt_at_ns);
        xmtp_mls_core::worker::tasks::test_support::run_and_reschedule_task(
            &TaskWorker::new(client.context.clone(), MlsTaskHandlers),
            task(),
        )
        .await?;
        assert_eq!(peer.calls(Call::Update), calls);
        peer.state.lock().limit = None;
        client
            .db()
            .update_task(refused.id, refused.attempts, before, before)?;
        xmtp_mls_core::worker::tasks::test_support::run_and_reschedule_task(
            &TaskWorker::new(client.context.clone(), MlsTaskHandlers),
            task(),
        )
        .await?;
        assert_eq!(task().attempts, 0);
    }

    /// A rate-limited add is not the topic limit: the state stays enabled, adds
    /// are not suppressed, and no request is sent before the delay ends.
    // verifies: PUSH-267, RATE-041
    #[xmtp_common::test(unwrap_try = true)]
    async fn notification_rate_limit_keeps_state_enabled_and_waits_the_delay() {
        use crate::task_handlers::MlsTaskHandlers;
        use xmtp_mls_core::worker::tasks::TaskWorker;
        let delay = Duration::from_secs(120);
        let mapped = NotificationError::from(xmtp_api::dyn_err(
            xmtp_proto::api::ApiClientError::client(xmtp_api_grpc::error::GrpcError::Status(
                xmtp_api_grpc::error::rate_limited_status("push", delay),
            )),
        ));
        assert!(matches!(mapped, NotificationError::Api(_)));
        assert!(mapped.is_retryable());

        let (client, peer) = support::client().await;
        let mut config = config();
        config.include_welcomes = false;
        client.enable_notifications(config).await?;
        run(&client.context).await?;
        client.create_group(None, None)?;
        wake(&client.context)?;
        let task = || {
            client
                .db()
                .get_tasks()
                .unwrap()
                .into_iter()
                .find(|row| row.data_hash == task_hash().as_ref())
                .unwrap()
        };
        let mut due = task();
        due.next_attempt_at_ns = 0;
        peer.state.lock().rate_limit = Some((Call::Update, delay));
        let calls = peer.calls(Call::Update);
        let before = time::now_ns();
        xmtp_mls_core::worker::tasks::test_support::run_and_reschedule_task(
            &TaskWorker::new(client.context.clone(), MlsTaskHandlers),
            due,
        )
        .await?;
        assert_eq!(peer.calls(Call::Update), calls + 1);
        let row = task();
        assert_eq!(row.attempts, 1);
        assert!(row.next_attempt_at_ns >= before + delay.as_nanos() as i64);
        assert!(matches!(
            client.notification_state()?,
            NotificationState::Enabled
        ));
        let record = client.db().notification_record()?;
        assert!(record.push_suppressed.is_none());
        assert!(record.push_failed_error.is_none());

        // A wake that makes the task run early still sends nothing inside the delay.
        let TaskOutcome::RescheduleAt(next) = run(&client.context).await? else {
            panic!("enabled task must remain");
        };
        assert_eq!(peer.calls(Call::Update), calls + 1);
        assert!(next >= before + delay.as_nanos() as i64);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn notification_successful_reenable_resets_old_retry_backoff() {
        use crate::task_handlers::MlsTaskHandlers;
        use xmtp_mls_core::worker::tasks::TaskWorker;
        let (client, peer) = support::client().await;
        client.create_group(None, None)?;
        client.enable_notifications(config()).await?;
        wake(&client.context)?;
        let task = || {
            client
                .db()
                .get_tasks()
                .unwrap()
                .into_iter()
                .find(|row| row.data_hash == task_hash().as_ref())
                .unwrap()
        };
        peer.state.lock().next_error = Some(tonic::Code::Unavailable);
        xmtp_mls_core::worker::tasks::test_support::run_and_reschedule_task(
            &TaskWorker::new(client.context.clone(), MlsTaskHandlers),
            task(),
        )
        .await?;
        assert_eq!(task().attempts, 1);
        assert!(peer.state.lock().subscriptions.is_empty());
        // Simulate an existing long retry without waiting through each failure.
        let retry_at = time::now_ns() + NS_IN_HOUR;
        let row = task();
        client
            .db()
            .update_task(row.id, 5, row.last_attempted_at_ns, retry_at)?;

        // Neither ordinary hints nor an unsuccessful enable bypass the retry.
        wake(&client.context)?;
        assert_eq!(task().next_attempt_at_ns, retry_at);
        peer.state.lock().next_error = Some(tonic::Code::Unavailable);
        assert!(client.enable_notifications(config()).await.is_err());
        wake(&client.context)?;
        assert_eq!(task().attempts, 5);
        assert_eq!(task().next_attempt_at_ns, retry_at);

        client.enable_notifications(config()).await?;
        // The runner consumes the successful registration hint.
        wake(&client.context)?;
        assert_eq!(task().attempts, 0);
        assert!(task().next_attempt_at_ns <= time::now_ns());
        xmtp_mls_core::worker::tasks::test_support::run_and_reschedule_task(
            &TaskWorker::new(client.context.clone(), MlsTaskHandlers),
            task(),
        )
        .await?;
        assert_eq!(peer.state.lock().subscriptions.len(), 2);
        assert_eq!(client.db().uploaded_topics()?.len(), 2);
        assert_eq!(peer.calls(Call::Update), 2);
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn notification_later_failure_cancels_registration_retry_reset() {
        use crate::task_handlers::MlsTaskHandlers;
        use xmtp_mls_core::worker::tasks::TaskWorker;
        let (client, peer) = support::client().await;
        client.enable_notifications(config()).await?;
        wake(&client.context)?;
        client.enable_notifications(config()).await?;
        // A due timer can run before the runner consumes the registration hint.
        let row = client
            .db()
            .get_tasks()?
            .into_iter()
            .find(|row| row.data_hash == task_hash().as_ref())
            .unwrap();
        peer.state.lock().next_error = Some(tonic::Code::Unavailable);
        xmtp_mls_core::worker::tasks::test_support::run_and_reschedule_task(
            &TaskWorker::new(client.context.clone(), MlsTaskHandlers),
            row,
        )
        .await?;
        let failed = client
            .db()
            .get_tasks()?
            .into_iter()
            .find(|row| row.data_hash == task_hash().as_ref())
            .unwrap();
        assert_eq!(failed.attempts, 1);
        wake(&client.context)?;
        let after = client
            .db()
            .get_tasks()?
            .into_iter()
            .find(|row| row.data_hash == task_hash().as_ref())
            .unwrap();
        assert_eq!(after.attempts, failed.attempts);
        assert_eq!(after.next_attempt_at_ns, failed.next_attempt_at_ns);
    }

    xmtp_common::if_native! {
        // verifies: PUSH-240
        #[xmtp_common::test(unwrap_try = true)]
        async fn notification_stalled_request_bounds_runner_delay_and_keeps_messaging_live() {
            use crate::task_handlers::MlsTaskHandlers;
            use xmtp_mls_identity::key_package_maintenance as kp;
    use xmtp_mls_core::worker::tasks::TaskWorker;
            const PHASE_DELAY: Duration = Duration::from_secs(20);
            const MAINTENANCE_WAIT: Duration = Duration::from_secs(42);

            for reset in [false, true] {
                let (client, peer) = support::client().await;
                tester!(bo, disable_workers);
                let group = client.create_group(None, None)?;
                client.enable_notifications(config()).await?;
                if reset {
                    peer.state.lock().next_error = Some(tonic::Code::Unavailable);
                    assert!(client.disable_notifications().await.unwrap_err().is_retryable());
                    peer.state.lock().next_error = Some(tonic::Code::Unavailable);
                    assert!(client.enable_notifications(config()).await.unwrap_err().is_retryable());
                    assert!(client.db().notification_record()?.push_unregister_owed);
                    assert!(matches!(client.notification_state()?, NotificationState::Enabled));
                }
                client.db().queue_key_package_rotation()?;
                let now = time::now_ns();
                client.db().create_or_ignore_task(kp::kp_seed(kp::kp_rotation_proto(), now + 5 * NS_IN_SEC)?)?;
                peer.state.lock().pause_next = true;
                let mut task_worker = TaskWorker::new(client.context.clone(), MlsTaskHandlers);
                let runner = xmtp_common::spawn(None, async move { task_worker.run().await });
                timeout(Duration::from_secs(5), peer.entered.notified()).await?;
                let entered_at = time::Instant::now();
                let phases = reset.then(|| {
                    let peer = peer.clone();
                    xmtp_common::spawn(None, async move {
                        time::sleep(PHASE_DELAY).await;
                        peer.state.lock().pause_next = true;
                        peer.release.notify_one();
                        timeout(Duration::from_secs(5), peer.entered.notified()).await.unwrap();
                        time::sleep(PHASE_DELAY).await;
                        peer.release.notify_one();
                    })
                });
                // Network waits cannot hold the database writer or the group state lock.
                timeout(Duration::from_secs(5), async {
                    group.update_consent_state(ConsentState::Denied)?;
                    group.send_message(b"notification wait does not block messages", Default::default()).await?;
                    let invited = bo.create_group(None, None)?;
                    invited.add_members(&[client.inbox_id()]).await?;
                    let welcomes = client.sync_welcomes().await?;
                    assert!(welcomes.iter().any(|item| item.group_id == invited.group_id));
                    Ok::<_, xmtp_mls_engine::groups::GroupError>(())
                }).await??;
                assert!(client.db().next_key_package_rotation_ns()?.unwrap() < now + NS_IN_HOUR);
                // Let both requests finish on the weaker code, then check the bound
                // from maintenance becoming due five seconds after the turn starts.
                timeout(MAINTENANCE_WAIT, async {
                    loop {
                        if client.db().next_key_package_rotation_ns().unwrap().unwrap() > now + NS_IN_HOUR { break; }
                        time::sleep(Duration::from_millis(50)).await;
                    }
                }).await?;
                let elapsed = entered_at.elapsed();
                assert!(elapsed < Duration::from_secs(35), "reset={reset}: key package maintenance waited {elapsed:?} after the notification turn started");
                assert!(peer.calls(Call::Publish) > 0);
                if reset {
                    assert_eq!(peer.calls(Call::Unregister), 3);
                    assert_eq!(peer.calls(Call::Register), 2);
                    let record = client.db().notification_record()?;
                    assert!(!record.push_unregister_owed);
                    assert!(record.push_last_state.is_none());
                    assert!(matches!(client.notification_state()?, NotificationState::Enabled));
                    assert!(!peer.state.lock().registered);
                }
                if let Some(phases) = phases {
                    phases.abort_handle().end();
                }
                runner.abort_handle().end();
                if reset {
                    run(&client.context).await?;
                    assert_eq!(peer.calls(Call::Unregister), 3);
                    assert_eq!(peer.calls(Call::Register), 3);
                    assert!(peer.state.lock().registered);
                    assert!(client.db().notification_record()?.push_last_state.is_some());
                }
            }
        }
    }
}

/// Notification contracts of a client's build and restart.
mod client {
    use crate::client::notifications::NotificationError;
    use crate::client::notifications::test_support::config;
    use crate::tester;
    use xmtp_db::prelude::*;

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
}
