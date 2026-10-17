use super::*;

// verifies: EVENT-022
// verifies: EVENT-030
// verifies: EVENT-031
#[xmtp_common::test(unwrap_try = true)]
async fn reader_counts_taken_event() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let reader = client
        .events(event_filter(vec![EventKind::HmacKeysUpdated]))
        .await?;
    emit_hmac(&client);
    let gate = Arc::new(reader::HandoffGate {
        arrived: Notify::new(),
        release: Notify::new(),
    });
    *reader.handoff_gate.lock() = Some(gate.clone());
    let read = reader.next();
    tokio::pin!(read);
    tokio::select! { _ = gate.arrived.notified() => {}, result = &mut read => panic!("read returned before handoff gate: {result:?}") }
    for _ in 0..1025 {
        emit_hmac(&client);
    }
    gate.release.notify_one();
    assert!(matches!(
        read.await?,
        Some(ClientEvent::HmacKeysUpdated { .. })
    ));
    for _ in 0..1023 {
        assert!(matches!(
            reader.next().await?,
            Some(ClientEvent::HmacKeysUpdated { .. })
        ));
    }
    assert!(matches!(
        reader.next().await?,
        Some(ClientEvent::Lagged { lagged }) if lagged.discarded == 2
    ));
    reader.end().await?;
    client.end().await?;
}

// verifies: EVENT-050
// verifies: EVENT-033
#[xmtp_common::test(unwrap_try = true)]
async fn listener_calls_are_sequential() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let release = Arc::new(Notify::new());
    let (probe, mut started) = event_probe(Some(release.clone()), false, None, false);
    let id = client
        .start_listener(
            event_filter(vec![EventKind::HmacKeysUpdated]),
            probe.clone(),
        )
        .await?;
    let other = client
        .events(event_filter(vec![EventKind::HmacKeysUpdated]))
        .await?;
    emit_hmac(&client);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), started.recv()).await?,
        Some(0)
    );
    emit_hmac(&client);
    assert!(matches!(
        other.next().await?,
        Some(ClientEvent::HmacKeysUpdated { .. })
    ));
    assert!(matches!(
        other.next().await?,
        Some(ClientEvent::HmacKeysUpdated { .. })
    ));
    let early = tokio::time::timeout(Duration::from_millis(20), started.recv()).await;
    tokio::time::sleep(Duration::from_millis(10)).await;
    release.notify_waiters();
    release.notify_one();
    assert!(early.is_err());
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), started.recv()).await?,
        Some(1)
    );
    assert_eq!(probe.maximum.load(Ordering::SeqCst), 1);
    client.stop_listener(id).await;
    release.notify_one();
    other.end().await?;
    client.end().await?;
}

// verifies: EVENT-051
#[xmtp_common::test(unwrap_try = true)]
async fn listener_failure_contained() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let (probe, mut started) = event_probe(None, true, None, false);
    let id = client
        .start_listener(event_filter(vec![EventKind::HmacKeysUpdated]), probe)
        .await?;
    emit_hmac(&client);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), started.recv()).await?,
        Some(0)
    );
    emit_hmac(&client);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), started.recv()).await?,
        Some(1)
    );
    client.stop_listener(id).await;
    client.end().await?;
}

// verifies: EVENT-052
#[xmtp_common::test(unwrap_try = true)]
async fn listener_reentrant_call_completes() {
    let client = Arc::new(Client::create(crate::generate_local_signer().await, options()).await?);
    let (probe, mut started) = event_probe(None, false, Some(client.clone()), false);
    let completed = probe.completed.clone();
    let id = client
        .start_listener(event_filter(vec![EventKind::HmacKeysUpdated]), probe)
        .await?;
    emit_hmac(&client);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), started.recv()).await?,
        Some(0)
    );
    tokio::time::timeout(Duration::from_secs(2), async {
        while !completed.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    client.stop_listener(id).await;
    client.end().await?;
}

// verifies: EVENT-053
#[xmtp_common::test(unwrap_try = true)]
async fn no_call_after_stop_returns() {
    let client = Arc::new(Client::create(crate::generate_local_signer().await, options()).await?);
    let (hook, release) = crate::events::dispatch::StartHook::new();
    client.listeners.set_start_hook_for_test(hook.clone());
    let (probe, mut started) = event_probe(None, false, None, false);
    let id = client
        .start_listener(event_filter(vec![EventKind::HmacKeysUpdated]), probe)
        .await?;
    emit_hmac(&client);
    tokio::time::timeout(Duration::from_secs(2), hook.arrived.notified()).await?;
    let stopping_client = client.clone();
    let runtime = tokio::runtime::Handle::current();
    let (attempted, ready) = std::sync::mpsc::channel();
    let stopping = std::thread::spawn(move || {
        let _ = attempted.send(());
        runtime.block_on(stopping_client.stop_listener(id));
    });
    ready.recv_timeout(Duration::from_secs(2))?;
    let stopping_client = client.clone();
    let runtime = tokio::runtime::Handle::current();
    let (attempted, ready) = std::sync::mpsc::channel();
    let stopping_again = std::thread::spawn(move || {
        let _ = attempted.send(());
        runtime.block_on(stopping_client.stop_listener(id));
    });
    ready.recv_timeout(Duration::from_secs(2))?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let stopped_before_release = stopping.is_finished() && stopping_again.is_finished();
    release.send(())?;
    assert!(
        tokio::time::timeout(
            Duration::from_secs(2),
            tokio::task::spawn_blocking(move || stopping.join().is_ok()),
        )
        .await??
    );
    assert!(
        tokio::time::timeout(
            Duration::from_secs(2),
            tokio::task::spawn_blocking(move || stopping_again.join().is_ok()),
        )
        .await??
    );
    assert!(
        stopped_before_release,
        "stop waited for a callback that had not started"
    );
    let first = tokio::time::timeout(Duration::from_millis(100), started.recv()).await;
    assert!(!matches!(first, Ok(Some(_))), "callback started after stop");
    emit_hmac(&client);
    let later = tokio::time::timeout(Duration::from_millis(100), started.recv()).await;
    assert!(!matches!(later, Ok(Some(_))));
    client.end().await?;
}

// verifies: EVENT-030
#[xmtp_common::test(unwrap_try = true)]
async fn blocked_listener_counts_running_event_in_queue_bound() {
    use tokio::sync::{Semaphore, mpsc};

    struct BoundListener {
        started: mpsc::UnboundedSender<ClientEvent>,
        release: Arc<Semaphore>,
    }

    #[xmtp_common::async_trait]
    impl EventListener for BoundListener {
        async fn on_event(&self, event: ClientEvent) -> Result<(), ListenerError> {
            let _ = self.started.send(event);
            self.release
                .acquire()
                .await
                .map_err(|_| ListenerError::Failed)?
                .forget();
            Ok(())
        }
    }

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let (started, mut events) = mpsc::unbounded_channel();
    let release = Arc::new(Semaphore::new(0));
    let id = client
        .start_listener(
            event_filter(vec![EventKind::HmacKeysUpdated]),
            Arc::new(BoundListener {
                started,
                release: release.clone(),
            }),
        )
        .await?;
    emit_hmac(&client);
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(2), events.recv()).await?,
        Some(ClientEvent::HmacKeysUpdated { .. })
    ));
    for _ in 0..1030 {
        emit_hmac(&client);
    }
    release.add_permits(1026);
    tokio::time::timeout(Duration::from_secs(30), async {
        for _ in 0..1023 {
            assert!(matches!(
                events.recv().await,
                Some(ClientEvent::HmacKeysUpdated { .. })
            ));
        }
        assert!(matches!(
            events.recv().await,
            Some(ClientEvent::Lagged { lagged }) if lagged.discarded == 7
        ));
    })
    .await?;
    client.stop_listener(id).await;
    client.end().await?;
}

// verifies: EVENT-054
#[xmtp_common::test(unwrap_try = true)]
async fn end_detaches_running_call() {
    let client = Arc::new(Client::create(crate::generate_local_signer().await, options()).await?);
    let release = Arc::new(Notify::new());
    let (probe, mut started) = event_probe(Some(release.clone()), false, None, false);
    let completed = probe.completed.clone();
    client
        .start_listener(event_filter(vec![EventKind::HmacKeysUpdated]), probe)
        .await?;
    emit_hmac(&client);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), started.recv()).await?,
        Some(0)
    );
    tokio::time::timeout(Duration::from_secs(2), client.end()).await??;
    assert!(!completed.load(Ordering::SeqCst));
    release.notify_one();
    tokio::time::timeout(Duration::from_secs(2), async {
        while !completed.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await?;
}

// verifies: EVENT-054
#[xmtp_common::test(unwrap_try = true)]
async fn end_racing_listener_start_leaves_no_listener() {
    let client = Arc::new(Client::create(crate::generate_local_signer().await, options()).await?);
    let (hook, release) = crate::events::dispatch::StartHook::new();
    client
        .listeners
        .set_registration_hook_for_test(hook.clone());
    let (probe, _) = event_probe(None, false, None, false);
    let starting_client = client.clone();
    let runtime = tokio::runtime::Handle::current();
    let start = std::thread::spawn(move || {
        runtime.block_on(
            starting_client.start_listener(event_filter(vec![EventKind::HmacKeysUpdated]), probe),
        )
    });
    tokio::time::timeout(Duration::from_secs(5), hook.arrived.notified()).await?;
    let end = tokio::time::timeout(Duration::from_secs(5), client.end()).await;
    release.send(())?;
    end??;
    let refused = tokio::task::spawn_blocking(move || {
        matches!(start.join(), Ok(Err(XmtpError::ClientClosed(_))))
    })
    .await?;
    assert!(refused, "listener started after client end");
    assert_eq!(client.listeners.active_count_for_test(), 0);
}

// verifies: EVENT-054
#[xmtp_common::test(unwrap_try = true)]
async fn end_racing_listener_stop_blocks_a_late_callback() {
    let client = Arc::new(Client::create(crate::generate_local_signer().await, options()).await?);
    let (start_hook, start_release) = crate::events::dispatch::StartHook::new();
    client.listeners.set_start_hook_for_test(start_hook.clone());
    let (probe, mut started) = event_probe(None, false, None, false);
    let id = client
        .start_listener(event_filter(vec![EventKind::HmacKeysUpdated]), probe)
        .await?;
    emit_hmac(&client);
    tokio::time::timeout(Duration::from_secs(5), start_hook.arrived.notified()).await?;

    let (stop_hook, stop_release) = crate::events::dispatch::StartHook::new();
    client.listeners.set_stop_hook_for_test(stop_hook.clone());
    let stopping_client = client.clone();
    let runtime = tokio::runtime::Handle::current();
    let stopping = std::thread::spawn(move || {
        runtime.block_on(stopping_client.stop_listener(id));
    });
    tokio::time::timeout(Duration::from_secs(5), stop_hook.arrived.notified()).await?;

    let end = tokio::time::timeout(Duration::from_secs(5), client.end()).await;
    start_release.send(())?;
    let late_callback = tokio::time::timeout(Duration::from_millis(100), started.recv()).await;
    stop_release.send(())?;
    assert!(
        tokio::task::spawn_blocking(move || stopping.join().is_ok()).await?,
        "stop thread failed"
    );
    end??;
    assert!(
        !matches!(late_callback, Ok(Some(_))),
        "listener callback started after client end"
    );
}
