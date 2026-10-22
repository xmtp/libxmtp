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

/// Hands each event it receives to the test.
struct ForwardingListener(tokio::sync::mpsc::UnboundedSender<ClientEvent>);

#[xmtp_common::async_trait]
impl EventListener for ForwardingListener {
    async fn on_event(&self, event: ClientEvent) -> Result<(), ListenerError> {
        let _ = self.0.send(event);
        Ok(())
    }
}

/// Allows the inbox whose ID is `fill` repeated, which emits one
/// `consent.changed` event for that entity on the writing client.
async fn allow_inbox(client: &Client, fill: &str) -> Result<(), XmtpError> {
    use crate::{ConsentEntity, ConsentRecord, ConsentState};

    client
        .preferences()
        .set_consent_states(vec![ConsentRecord {
            entity: ConsentEntity::Inbox {
                inbox_id: InboxId::try_from(fill.repeat(64))?,
            },
            state: ConsentState::Allowed,
        }])
        .await
}

fn consent_entity(event: Option<ClientEvent>) -> String {
    match event {
        Some(ClientEvent::ConsentChanged { consent_changed }) => consent_changed.entity,
        other => panic!("expected consent.changed, got {other:?}"),
    }
}

/// Each client writes in turn. A listener and a reader on each client must
/// see only that client's writes, in order, and nothing after them.
async fn assert_events_stay_with_their_client(first: &Client, second: &Client) {
    let filter = || event_filter(vec![EventKind::ConsentChanged]);
    let (first_sender, mut first_heard) = tokio::sync::mpsc::unbounded_channel();
    let (second_sender, mut second_heard) = tokio::sync::mpsc::unbounded_channel();
    let first_listener = first
        .start_listener(filter(), Arc::new(ForwardingListener(first_sender)))
        .await
        .expect("first listener");
    let second_listener = second
        .start_listener(filter(), Arc::new(ForwardingListener(second_sender)))
        .await
        .expect("second listener");
    let first_reader = first.events(filter()).await.expect("first reader");
    let second_reader = second.events(filter()).await.expect("second reader");
    let wait = Duration::from_secs(5);
    for (mine, theirs) in [("a", "b"), ("c", "d")] {
        allow_inbox(first, mine).await.expect("first write");
        allow_inbox(second, theirs).await.expect("second write");
        for (heard, reader, fill) in [
            (&mut first_heard, &first_reader, mine),
            (&mut second_heard, &second_reader, theirs),
        ] {
            let expected = fill.repeat(64);
            let listened = tokio::time::timeout(wait, heard.recv())
                .await
                .expect("listener event in time");
            assert_eq!(consent_entity(listened), expected, "listener event");
            let read = tokio::time::timeout(wait, reader.next())
                .await
                .expect("reader event in time")
                .expect("reader event");
            assert_eq!(consent_entity(read), expected, "reader event");
        }
    }
    let quiet = Duration::from_millis(200);
    for (heard, reader) in [
        (&mut first_heard, &first_reader),
        (&mut second_heard, &second_reader),
    ] {
        let late = tokio::time::timeout(quiet, heard.recv()).await;
        assert!(late.is_err(), "listener got another event: {late:?}");
        let late = tokio::time::timeout(quiet, reader.next()).await;
        assert!(late.is_err(), "reader got another event: {late:?}");
    }
    first.stop_listener(first_listener).await;
    second.stop_listener(second_listener).await;
    first_reader.end().await.expect("end first reader");
    second_reader.end().await.expect("end second reader");
}

// Two client instances in one process open the same database file with the
// same identity.
// verifies: EVENT-056
#[xmtp_common::test(unwrap_try = true)]
async fn clients_sharing_a_database_get_only_their_own_events() {
    let root = temp_root("shared-database-events");
    std::fs::create_dir_all(&root)?;
    let mut settings = options();
    settings.storage.location = explicit_location(&root.join("shared.db3"));
    let signer = crate::generate_local_signer().await;
    let first = Client::create(signer.clone(), settings.clone()).await?;
    let second = Client::create(signer, settings).await?;
    assert_eq!(first.inbox_id(), second.inbox_id());
    assert_eq!(
        first.storage().path().await?,
        second.storage().path().await?
    );
    assert_events_stay_with_their_client(&first, &second).await;
    first.end().await?;
    second.end().await?;
    std::fs::remove_dir_all(root)?;
}

// Two client instances in one process share one connected backend, and with
// it one credential handle.
// verifies: EVENT-056
#[xmtp_common::test(unwrap_try = true)]
async fn clients_sharing_a_credential_get_only_their_own_events() {
    struct FixedCredential;

    #[xmtp_common::async_trait]
    impl CredentialSource for FixedCredential {
        async fn credential(&self) -> Result<Credential, CredentialError> {
            Ok(Credential {
                name: None,
                value: "shared-event-credential".into(),
                expires_at_seconds: xmtp_common::time::now_secs() + 300,
            })
        }
    }

    let backend = Arc::new(
        crate::Backend::connect(BackendOptions {
            url: xmtp_configuration::backend_test_url(),
            credentials: Some(Arc::new(FixedCredential)),
            ..Default::default()
        })
        .await?,
    );
    let mut settings = options();
    settings.backend = Some(BackendSource::Connected { backend });
    let first = Client::create(crate::generate_local_signer().await, settings.clone()).await?;
    let second = Client::create(crate::generate_local_signer().await, settings).await?;
    assert_events_stay_with_their_client(&first, &second).await;
    first.end().await?;
    second.end().await?;
}
