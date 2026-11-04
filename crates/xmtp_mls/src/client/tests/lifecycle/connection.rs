//! Client lifecycle tests: connection.

use super::*;

#[xmtp_common::timeout(Duration::from_secs(100))]
#[xmtp_common::test(unwrap_try = true)]
#[cfg(not(target_arch = "wasm32"))]
/// One conversation stream resumes from durable receipt after a black hole.
async fn should_reconnect() {
    use crate::subscriptions::incoming::{IncomingConnection, IncomingCoordinator, IncomingScope};
    use futures::FutureExt;
    use std::panic::AssertUnwindSafe;
    use xmtp_db::incoming_envelope::{NetworkEntityKind, QueryIncomingEnvelope, StreamTopic};
    use xmtp_proto::types::Topic;

    // Nextest starts a fresh process, so these values precede the first client.
    unsafe {
        std::env::set_var("XMTP_GRPC_KEEPALIVE_INTERVAL_SECS", "5");
        std::env::set_var("XMTP_GRPC_KEEPALIVE_TIMEOUT_SECS", "5");
    }
    toxiproxy_test(async || {
        tester!(alix, proxy, disable_workers);
        tester!(bo, disable_workers);
        let start_new_convo = || async {
            bo.create_group_with_members(&[alix.inbox_id().to_string()], None, None)
                .await
                .unwrap()
        };
        let stream = alix.stream_conversations(None, false).await.unwrap();
        futures::pin_mut!(stream);
        let lease =
            IncomingCoordinator::for_context(&alix.context).acquire(IncomingScope::Topics(vec![
                Topic::new_welcome_message(alix.installation_public_key()),
            ]));
        let topic = StreamTopic {
            entity_id: alix.installation_public_key().to_vec(),
            kind: NetworkEntityKind::Welcome,
        };
        let wait = Duration::from_secs(20);

        let initial = start_new_convo().await;
        let delivered = xmtp_common::time::timeout(wait, stream.try_next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(delivered.group_id, initial.group_id);
        let connected = xmtp_common::wait_for_some(|| async {
            let status = lease.snapshot();
            (status.connection == IncomingConnection::Connected).then_some(status)
        })
        .await
        .expect("the initial receiver did not connect");
        let before = alix.context.db().topic_progress(&topic).unwrap();
        assert!(before.processed.0 > 0);
        assert_eq!(before.received, before.processed);

        alix.for_each_proxy(async |p| {
            p.with_timeout("downstream".into(), 60_000, 1.0)
                .await
                .unwrap();
        })
        .await;
        let outage = AssertUnwindSafe(async {
            let missed = start_new_convo().await;
            assert!(
                xmtp_common::wait_for_some(|| async {
                    (lease.snapshot().connection != IncomingConnection::Connected).then_some(())
                })
                .await
                .is_some(),
                "the receiver did not detect the black hole"
            );
            let pending = alix.context.db().topic_progress(&topic).unwrap();
            assert_eq!(pending.received, before.received);
            assert_eq!(pending.processed, before.processed);
            missed
        })
        .catch_unwind()
        .await;
        // Restore the test's proxy before reporting any outage assertion failure.
        alix.for_each_proxy(async |p| {
            p.delete_all_toxics().await.unwrap();
        })
        .await;
        let missed = outage.unwrap();
        let delivered = xmtp_common::time::timeout(wait, stream.try_next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(delivered.group_id, missed.group_id);
        assert!(
            xmtp_common::wait_for_some(|| async {
                let status = lease.snapshot();
                (status.connection == IncomingConnection::Connected
                    && status.connection_generation > connected.connection_generation)
                    .then_some(())
            })
            .await
            .is_some(),
            "the original receiver did not reconnect"
        );
        let recovered = alix.context.db().topic_progress(&topic).unwrap();
        assert!(recovered.processed > before.processed);
        assert_eq!(recovered.received, recovered.processed);

        let fresh = start_new_convo().await;
        let delivered = xmtp_common::time::timeout(wait, stream.try_next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(delivered.group_id, fresh.group_id);
        let after = alix.context.db().topic_progress(&topic).unwrap();
        assert!(after.processed > recovered.processed);
        assert_eq!(after.received, after.processed);
    })
    .await
}

#[xmtp_common::test(unwrap_try = true)]
async fn close_stops_workers() {
    tester!(client);
    assert!(
        client.workers.is_running(),
        "worker supervisor must be running before close"
    );

    client.close().await?;

    assert!(
        !client.workers.is_running(),
        "supervisor handle should be taken after close"
    );
    assert!(
        client.context.is_closed(),
        "context closed flag must be set after close"
    );
    assert!(
        client.context.cancellation_token().is_cancelled(),
        "cancellation token must be cancelled after close"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn reconnect_keeps_worker_subscription_open_for_queued_facts() {
    use crate::subscriptions::internal::InternalEvent;
    use crate::worker::WorkerKind;
    use std::sync::Arc;
    use tokio::sync::Notify;
    use xmtp_events::EventWriter;

    tester!(alix, persistent_db);
    let subscription = alix
        .client
        .workers
        .subscription_for_test(WorkerKind::TaskRunner)
        .expect("TaskRunner has a subscription");
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    crate::worker::test_hooks::pause_next_spawn(
        alix.installation_id.to_vec(),
        entered.clone(),
        release.clone(),
    );
    alix.client.reconnect_db()?;
    xmtp_common::time::timeout(std::time::Duration::from_secs(10), entered.notified()).await?;
    xmtp_common::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(!subscription.is_closed());

    let (consumed, received) = tokio::sync::oneshot::channel();
    crate::worker::tasks::test_hooks::on_task_scheduled_consumed(
        alix.installation_id.to_vec(),
        consumed,
    );
    alix.context
        .events()
        .emit(None, Some(InternalEvent::TaskScheduled));
    release.notify_one();
    xmtp_common::time::timeout(std::time::Duration::from_secs(10), received).await??;
    assert!(!subscription.is_closed());
}

#[xmtp_common::test(unwrap_try = true)]
async fn clients_sharing_a_database_receive_only_their_own_events() {
    use xmtp_events::{EventFilter, EventKind};

    tester!(alix, disable_workers);
    let second = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .with_disable_workers(true)
        .build()
        .await?;
    let first_events = alix
        .context
        .events()
        .subscribe(EventFilter::new([EventKind::ConsentChanged]), Some(10));
    let second_events = second
        .context
        .events()
        .subscribe(EventFilter::new([EventKind::ConsentChanged]), Some(10));

    alix.set_consent_states(&[crate::client::ConsentAct::new(
        ConsentType::InboxId,
        ConsentState::Allowed,
        "from-first-client".into(),
    )])
    .await?;
    assert_eq!(first_events.drain().len(), 1);
    assert!(second_events.drain().is_empty());

    second
        .set_consent_states(&[crate::client::ConsentAct::new(
            ConsentType::InboxId,
            ConsentState::Denied,
            "from-second-client".into(),
        )])
        .await?;
    assert!(first_events.drain().is_empty());
    assert_eq!(second_events.drain().len(), 1);
}

#[xmtp_common::test(unwrap_try = true)]
async fn close_cleans_up_workers_when_delivery_release_fails() {
    use crate::worker::WorkerKind;

    tester!(alix);
    let subscription = alix
        .client
        .workers
        .subscription_for_test(WorkerKind::TaskRunner)
        .expect("TaskRunner has a subscription");
    crate::context::test_hooks::fail_next_delivery_release(alix.installation_id.to_vec());

    assert!(alix.close().await.is_err());
    assert!(!alix.client.workers.is_running());
    assert!(subscription.is_closed());
    assert!(!alix.context.shutdown_complete());
    assert!(alix.close().await.is_ok());
    assert!(alix.context.shutdown_complete());
}

#[xmtp_common::test(unwrap_try = true)]
async fn close_is_idempotent() {
    tester!(client);
    client.close().await?;
    // second call must return Ok(()) without panic
    client.close().await?;
}

// persistent_db: ephemeral in-memory stores no-op on disconnect, so the
// pool-released assertion only meaningfully tests against a real SQLite
// file. Skipped on WASM where file-backed test stores aren't wired in.
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn close_disconnects_db() {
    use diesel::RunQueryDsl;
    use diesel::sql_query;

    tester!(client, persistent_db);
    client.close().await?;

    let conn = client.context.store().conn();
    let result = conn.raw_query(|c| sql_query("SELECT 1").execute(c));
    assert!(
        result.is_err(),
        "raw_query after close should surface a ConnectionError; got Ok"
    );
}

#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn close_cancels_callback_stream() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    tester!(client);

    let closed_flag = Arc::new(AtomicBool::new(false));
    let flag_for_cb = closed_flag.clone();

    let _handle = crate::client::Client::stream_conversations_with_callback(
        Arc::new((*client).clone()),
        None,
        move |_| {},
        move || {
            flag_for_cb.store(true, Ordering::SeqCst);
        },
        false,
    );

    client.close().await?;

    xmtp_common::time::timeout(std::time::Duration::from_secs(1), async {
        while !closed_flag.load(Ordering::SeqCst) {
            xmtp_common::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("on_close must fire within 1s of Client::close");
}

// verifies: STORE-017
// A reconnect that passed its closed check before close began starts workers.
// Close must wait for it and then stop those workers, so no worker outlives
// close.
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread", worker_threads = 2)]
#[cfg(not(target_arch = "wasm32"))]
async fn close_stops_workers_started_by_a_running_reconnect() {
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::Barrier;

    tester!(alix, persistent_db);
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    *crate::client::test_hooks::PAUSE_RECONNECT.lock() = Some((
        alix.installation_id.to_vec(),
        entered.clone(),
        release.clone(),
    ));
    let reconnecting_client = alix.client.clone();
    let reconnecting = tokio::task::spawn_blocking(move || reconnecting_client.reconnect_db());
    xmtp_common::time::timeout(Duration::from_secs(10), entered.wait()).await?;

    let closing_client = alix.client.clone();
    let mut closing = tokio::spawn(async move { closing_client.close().await });
    // Close refuses new calls once it waits for the running ones.
    xmtp_common::time::timeout(Duration::from_secs(10), async {
        while alix.context.foreground_calls().enter().is_some() {
            xmtp_common::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    assert!(
        tokio::time::timeout(Duration::from_millis(200), &mut closing)
            .await
            .is_err(),
        "close finished while a reconnect was running"
    );

    release.wait().await;
    xmtp_common::time::timeout(Duration::from_secs(10), reconnecting).await???;
    xmtp_common::time::timeout(Duration::from_secs(10), closing).await???;
    let worker_outlived_close = alix.client.workers.is_running();
    alix.client.workers.shutdown().await;
    assert!(
        !worker_outlived_close,
        "a worker supervisor started by reconnect outlived close"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn reconnect_after_close_errors() {
    tester!(client);
    client.close().await?;

    let err = client
        .reconnect_db()
        .expect_err("reconnect_db after close must fail");
    assert!(
        matches!(err, crate::client::ClientError::AlreadyClosed),
        "expected ClientError::AlreadyClosed, got {err:?}"
    );
}
