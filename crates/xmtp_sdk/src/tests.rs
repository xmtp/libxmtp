#![cfg(not(target_arch = "wasm32"))]

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::{future::Future, time::Duration};

use alloy::signers::local::PrivateKeySigner;
use tokio::sync::Notify;
use xmtp_db::{group::GroupQueryArgs, group_message::MsgQueryArgs};
use xmtp_id::{InboxOwner, associations::unverified::UnverifiedSignature};
use xmtp_mls::context::XmtpSharedContext;
use xmtp_mls::subscriptions::local_delivery::LocalDeliveryError;

use crate::{
    BackendOptions, BackendSource, Client, ClientOptions, ConversationID, Credential,
    CredentialError, CredentialSource, InboxID, MessageContent, MessageID, PublicIdentity,
    PublicIdentityKind, Signature, Signer, SignerError, SignerKind, SigningRequest,
    StorageLocation, StorageOptions, XmtpError, client::native_storage_path,
    credentials::AuthBridge, reader, signer,
};

use crate::{ClientEvent, EventFilter, EventKind, EventListener, ListenerError};
use xmtp_events::{EventWriter, HmacKeysUpdated};

fn event_filter(kinds: Vec<EventKind>) -> EventFilter {
    EventFilter {
        kinds,
        ..EventFilter::default()
    }
}

fn emit_hmac(client: &Client) {
    client.inner.context.events().emit(
        Some(xmtp_events::ClientEvent::HmacKeysUpdated(HmacKeysUpdated)),
        None,
    );
}

struct EventProbe {
    started: tokio::sync::mpsc::UnboundedSender<usize>,
    completed: Arc<AtomicBool>,
    release: Option<Arc<Notify>>,
    calls: std::sync::atomic::AtomicUsize,
    active: std::sync::atomic::AtomicUsize,
    maximum: std::sync::atomic::AtomicUsize,
    fail_first: bool,
    reenter: Option<Arc<Client>>,
    end_inside: bool,
}

#[xmtp_common::async_trait]
impl EventListener for EventProbe {
    async fn on_event(&self, _event: ClientEvent) -> Result<(), ListenerError> {
        let index = self.calls.fetch_add(1, Ordering::SeqCst);
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.maximum.fetch_max(active, Ordering::SeqCst);
        let _ = self.started.send(index);
        if let Some(client) = &self.reenter {
            if self.end_inside {
                client.end().await.map_err(|_| ListenerError::Failed)?;
            } else {
                let reader = client
                    .events(event_filter(vec![EventKind::HmacKeysUpdated]))
                    .await
                    .map_err(|_| ListenerError::Failed)?;
                reader.end().await.map_err(|_| ListenerError::Failed)?;
            }
        }
        if let Some(release) = &self.release {
            release.notified().await;
        }
        self.completed.store(true, Ordering::SeqCst);
        self.active.fetch_sub(1, Ordering::SeqCst);
        if index == 0 && self.fail_first {
            Err(ListenerError::Failed)
        } else {
            Ok(())
        }
    }
}

fn event_probe(
    release: Option<Arc<Notify>>,
    fail_first: bool,
    reenter: Option<Arc<Client>>,
    end_inside: bool,
) -> (Arc<EventProbe>, tokio::sync::mpsc::UnboundedReceiver<usize>) {
    let (started, receiver) = tokio::sync::mpsc::unbounded_channel();
    (
        Arc::new(EventProbe {
            started,
            completed: Arc::new(AtomicBool::new(false)),
            release,
            calls: std::sync::atomic::AtomicUsize::new(0),
            active: std::sync::atomic::AtomicUsize::new(0),
            maximum: std::sync::atomic::AtomicUsize::new(0),
            fail_first,
            reenter,
            end_inside,
        }),
        receiver,
    )
}

// verifies: EVENT-014
// verifies: EVENT-015
#[xmtp_common::test(unwrap_try = true)]
async fn events_registered_before_return() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    emit_hmac(&client);
    let reader = client
        .events(event_filter(vec![
            EventKind::HmacKeysUpdated,
            EventKind::ArchiveRestored,
        ]))
        .await?;
    emit_hmac(&client);
    client.inner.context.events().emit(
        Some(xmtp_events::ClientEvent::ArchiveRestored(
            xmtp_events::ArchiveRestored { complete: true },
        )),
        None,
    );
    assert!(matches!(
        reader.next().await?,
        Some(ClientEvent::HmacKeysUpdated)
    ));
    assert!(matches!(
        reader.next().await?,
        Some(ClientEvent::ArchiveRestored { complete: true })
    ));
    reader.end().await?;
    client.end().await?;
}

// verifies: EVENT-013
#[xmtp_common::test(unwrap_try = true)]
async fn event_reader_and_listener_create_no_network_interest() {
    use xmtp_proto::api::HasStats;

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let api = &client.inner.context.api().api_client;
    let mls = api.as_ref().mls_stats();
    let identity = api.as_ref().identity_stats();
    let api_counts = || {
        [
            mls.publish.get_count(),
            mls.query.get_count(),
            mls.query_newest.get_count(),
            mls.subscribe.get_count(),
            mls.subscribe_static.get_count(),
            identity.get_inbox_ids.get_count(),
            identity.verify_smart_contract_wallet_signatures.get_count(),
        ]
    };
    let lease_count = || {
        client
            .inner
            .context
            .incoming_runtime()
            .active_lease_count_for_test()
    };
    let baseline = (api_counts(), lease_count());

    let reader = client
        .events(event_filter(vec![EventKind::HmacKeysUpdated]))
        .await?;
    assert_eq!((api_counts(), lease_count()), baseline, "reader start");
    let (listener, mut started) = event_probe(None, false, None, false);
    let listener_id = client
        .start_listener(event_filter(vec![EventKind::HmacKeysUpdated]), listener)
        .await?;
    assert_eq!((api_counts(), lease_count()), baseline, "listener start");

    emit_hmac(&client);
    assert!(matches!(
        reader.next().await?,
        Some(ClientEvent::HmacKeysUpdated)
    ));
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), started.recv()).await?,
        Some(0)
    );
    tokio::time::sleep(Duration::from_millis(25)).await;
    assert_eq!(
        (api_counts(), lease_count()),
        baseline,
        "held subscriptions"
    );

    client.stop_listener(listener_id).await;
    reader.end().await?;
    assert_eq!((api_counts(), lease_count()), baseline, "subscription end");
    client.end().await?;
}

// verifies: EVENT-016
// verifies: EVENT-053
// verifies: EVENT-054
#[xmtp_common::test(unwrap_try = true)]
async fn event_reader_stays_open_on_rejection_and_ends_on_close() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let reader = client
        .events(event_filter(vec![EventKind::ClientRejectedByServer]))
        .await?;
    client.inner.context.cancellation_token().cancel();
    client.inner.context.events().emit(
        Some(xmtp_events::ClientEvent::ClientRejectedByServer(
            xmtp_events::ClientRejectedByServer {
                cause: xmtp_events::RejectionCause::BackendMismatch,
                min_libxmtp_version: None,
            },
        )),
        None,
    );
    assert!(matches!(
        reader.next().await?,
        Some(ClientEvent::ClientRejectedByServer { .. })
    ));
    let reading = reader.clone();
    let mut pending = tokio::spawn(async move { reading.next().await });
    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut pending)
            .await
            .is_err()
    );
    client.end().await?;
    assert!(pending.await??.is_none());
    assert!(matches!(
        client.events(EventFilter::default()).await,
        Err(XmtpError::ClientClosed(_))
    ));
}

// verifies: EVENT-053
#[xmtp_common::test(unwrap_try = true)]
async fn event_reader_end_waits_for_in_flight_read() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let reader = client
        .events(event_filter(vec![EventKind::HmacKeysUpdated]))
        .await?;
    let gate = Arc::new(reader::HandoffGate {
        arrived: Notify::new(),
        release: Notify::new(),
    });
    *reader.handoff_gate.lock() = Some(gate.clone());
    emit_hmac(&client);
    let reading = reader.clone();
    let read = tokio::spawn(async move { reading.next().await });
    xmtp_common::time::timeout(Duration::from_secs(5), gate.arrived.notified()).await?;
    let ending = reader.clone();
    let mut end = tokio::spawn(async move { ending.end().await });
    let ended_before_read = tokio::time::timeout(Duration::from_millis(50), &mut end)
        .await
        .is_ok();
    gate.release.notify_one();
    assert!(!ended_before_read, "end returned during an in-flight read");
    assert!(read.await??.is_none());
    end.await??;
    client.end().await?;
}

// verifies: EVENT-054
#[xmtp_common::test(unwrap_try = true)]
async fn client_end_waits_for_in_flight_event_read() {
    let client = Arc::new(Client::create(crate::generate_local_signer().await, options()).await?);
    let reader = client
        .events(event_filter(vec![EventKind::HmacKeysUpdated]))
        .await?;
    let gate = Arc::new(reader::HandoffGate {
        arrived: Notify::new(),
        release: Notify::new(),
    });
    *reader.handoff_gate.lock() = Some(gate.clone());
    emit_hmac(&client);
    let reading = reader.clone();
    let read = tokio::spawn(async move { reading.next().await });
    tokio::time::timeout(Duration::from_secs(5), gate.arrived.notified()).await?;
    let ending = client.clone();
    let mut end = tokio::spawn(async move { ending.end().await });
    let ended_before_read = tokio::time::timeout(Duration::from_millis(50), &mut end)
        .await
        .is_ok();
    gate.release.notify_one();
    assert!(
        !ended_before_read,
        "client end returned during an event read"
    );
    assert!(read.await??.is_none());
    end.await??;
}

// verifies: EVENT-020
// verifies: EVENT-021
#[xmtp_common::test(unwrap_try = true)]
async fn event_filter_selects_before_queueing() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let filter = EventFilter {
        content_types: Some(vec![crate::ContentTypeId {
            authority_id: "xmtp.org".into(),
            type_id: "reply".into(),
            version_major: 1,
            version_minor: 9,
        }]),
        references_own_messages: true,
        ..event_filter(vec![EventKind::MessageReceived])
    };
    let reader = client.events(filter).await?;
    let bus = client.inner.context.events();
    let message = xmtp_events::ClientEvent::MessageReceived(xmtp_events::MessageReceived {
        group_id: vec![1; 16],
        message_id: vec![2; 32],
        content_type: Some(xmtp_events::ContentTypeId {
            authority_id: "xmtp.org".into(),
            type_id: "reply".into(),
            version_major: 1,
        }),
        sender_inbox_id: client.inbox_id().0,
    });
    bus.emit(Some(message.clone()), None);
    bus.emit_with_context(
        Some(message),
        None,
        xmtp_events::EventContext {
            references_own_messages: true,
            ..Default::default()
        },
    );
    assert!(matches!(
        reader.next().await?,
        Some(ClientEvent::MessageReceived { .. })
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(20), reader.next())
            .await
            .is_err()
    );
    reader.end().await?;
    client.end().await?;
}

// verifies: EVENT-020
#[xmtp_common::test(unwrap_try = true)]
async fn event_filter_matches_stitched_dm_identifier() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let other = Client::create(crate::generate_local_signer().await, options()).await?;
    let dm = client
        .inner
        .find_or_create_dm(other.inbox_id().0, None)
        .await?;
    let dm_identifier = dm.dm_id.clone().expect("DM ID");
    let reader = client
        .events(EventFilter {
            conversation_ids: Some(vec![dm.group_id.into()]),
            ..event_filter(vec![EventKind::ConversationJoined])
        })
        .await?;
    let event = xmtp_events::ClientEvent::ConversationJoined(xmtp_events::ConversationJoined {
        group_id: vec![8; 16],
        conversation_type: xmtp_events::ConversationType::Dm,
        origin: xmtp_events::JoinOrigin::Welcomed,
        adder_inbox_id: None,
    });
    client
        .inner
        .context
        .events()
        .emit(Some(event.clone()), None);
    client.inner.context.events().emit_with_context(
        Some(event),
        None,
        xmtp_events::EventContext {
            dm_identifier: Some(dm_identifier.into_bytes()),
            ..Default::default()
        },
    );
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(2), reader.next()).await??,
        Some(ClientEvent::ConversationJoined { .. })
    ));
    assert!(
        tokio::time::timeout(Duration::from_millis(20), reader.next())
            .await
            .is_err()
    );
    reader.end().await?;
    other.end().await?;
    client.end().await?;
}

// verifies: EVENT-020
#[xmtp_common::test(unwrap_try = true)]
async fn consent_event_for_stitched_dm_reaches_group_filter() {
    use xmtp_db::consent_record::{ConsentState, ConsentType, StoredConsentRecord};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let other = Client::create(crate::generate_local_signer().await, options()).await?;
    let dm_a = client
        .inner
        .find_or_create_dm(other.inbox_id().0, None)
        .await?;
    let dm_b = other
        .inner
        .find_or_create_dm(client.inbox_id().0, None)
        .await?;
    assert_ne!(dm_a.group_id, dm_b.group_id);
    client.inner.sync_welcomes().await?;
    let stitched = client.inner.group(&dm_b.group_id)?;
    assert_eq!(dm_a.dm_id, stitched.dm_id);

    let reader = client
        .events(EventFilter {
            conversation_ids: Some(vec![dm_a.group_id.into()]),
            ..event_filter(vec![EventKind::ConsentChanged])
        })
        .await?;
    let entity = hex::encode(dm_b.group_id);
    client
        .inner
        .set_consent_states(&[StoredConsentRecord::new(
            ConsentType::ConversationId,
            ConsentState::Denied,
            entity.clone(),
        )])
        .await?;
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(5), reader.next()).await??,
        Some(ClientEvent::ConsentChanged { entity: received, .. }) if received == entity
    ));
    reader.end().await?;
    other.end().await?;
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn event_filter_accepts_unknown_conversation_id() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let unknown = xmtp_proto::types::GroupId::from([0xee; 16]);
    let reader = client
        .events(EventFilter {
            conversation_ids: Some(vec![unknown.into()]),
            ..event_filter(vec![EventKind::ConversationJoined])
        })
        .await?;
    reader.end().await?;
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn event_filter_reports_storage_error_when_resolving_dm() {
    use xmtp_db::ConnectionExt;

    let mut settings = options();
    let path = std::env::temp_dir().join(format!(
        "xmtp-sdk-event-filter-{}-{}.db3",
        std::process::id(),
        xmtp_common::time::now_ns()
    ));
    settings.storage.location = StorageLocation::Path(path.to_string_lossy().into_owned());
    let client = Client::create(crate::generate_local_signer().await, settings).await?;
    let other = Client::create(crate::generate_local_signer().await, options()).await?;
    let dm = client
        .inner
        .find_or_create_dm(other.inbox_id().0, None)
        .await?;
    client.inner.context.db().disconnect()?;
    let result = client
        .events(EventFilter {
            conversation_ids: Some(vec![dm.group_id.into()]),
            ..event_filter(vec![EventKind::ConversationJoined])
        })
        .await;
    client.inner.context.db().reconnect()?;
    assert!(result.is_err(), "storage error silently dropped the DM ID");
    other.end().await?;
    client.end().await?;
}

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
    assert!(matches!(read.await?, Some(ClientEvent::HmacKeysUpdated)));
    for _ in 0..1023 {
        assert!(matches!(
            reader.next().await?,
            Some(ClientEvent::HmacKeysUpdated)
        ));
    }
    assert!(matches!(
        reader.next().await?,
        Some(ClientEvent::Lagged { discarded: 2 })
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
        Some(ClientEvent::HmacKeysUpdated)
    ));
    assert!(matches!(
        other.next().await?,
        Some(ClientEvent::HmacKeysUpdated)
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
        Some(ClientEvent::HmacKeysUpdated)
    ));
    for _ in 0..1030 {
        emit_hmac(&client);
    }
    release.add_permits(1026);
    tokio::time::timeout(Duration::from_secs(30), async {
        for _ in 0..1023 {
            assert!(matches!(
                events.recv().await,
                Some(ClientEvent::HmacKeysUpdated)
            ));
        }
        assert!(matches!(
            events.recv().await,
            Some(ClientEvent::Lagged { discarded: 7 })
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
    let stopping = std::thread::spawn(move || runtime.block_on(stopping_client.stop_listener(id)));
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

#[xmtp_common::test(unwrap_try = true)]
async fn local_signer_and_signature_request_register() {
    assert!(matches!(
        crate::local_signer_from_private_key(vec![0; 31]).await,
        Err(XmtpError::InvalidInput(_))
    ));
    let signer = crate::generate_local_signer().await;
    let mut settings = options();
    settings.registration.auto = false;
    let client = Client::create(signer.clone(), settings).await?;
    assert!(!client.is_registered().await?);
    let request = client
        .unsafe_create_inbox_signature_request()
        .await?
        .expect("new inbox request");
    assert!(!request.signature_text().await.is_empty());
    request.sign(signer).await?;
    client.unsafe_apply_signature_request(request).await?;
    assert!(client.is_registered().await?);
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn added_account_opens_the_existing_inbox() {
    let owner = Client::create(crate::generate_local_signer().await, options()).await?;
    let second_signer = crate::generate_local_signer().await;
    owner
        .unsafe_add_account(second_signer.clone(), false)
        .await?;
    let identifier = signer::identity(second_signer.clone()).await?.to_core()?;
    let backend = options().backend.unwrap_or_default().resolve().await?;
    let api = xmtp_api::ApiClientWrapper::new(backend.api.clone(), Default::default());
    let expected = owner.inbox_id().0;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let found = api
                .get_inbox_ids(vec![identifier.clone().into()])
                .await
                .map_err(XmtpError::from_api)?;
            if found.into_iter().next().flatten().as_deref() == Some(expected.as_str()) {
                return Ok::<(), XmtpError>(());
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("added account did not become visible to the backend")?;
    let second = Client::create(second_signer, options()).await?;
    assert_eq!(second.inbox_id(), owner.inbox_id());
    assert!(owner.inbox_state(true).await?.identities.len() >= 2);
    second.end().await?;
    owner.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn encryption_round_trips_and_rejects_changed_bytes() {
    let plaintext = b"sdk attachment".to_vec();
    let encrypted = crate::crypto::encrypt_bytes(plaintext.clone()).await?;
    assert_eq!(
        crate::crypto::decrypt_bytes(encrypted.ciphertext.clone(), encrypted.keys.clone()).await?,
        plaintext
    );
    let mut changed = encrypted.ciphertext;
    changed[0] ^= 1;
    assert!(
        crate::crypto::decrypt_bytes(changed, encrypted.keys)
            .await
            .is_err()
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn encoded_content_encryption_rejects_missing_content_type() {
    use prost::Message as _;
    use xmtp_content_types::ContentCodec;
    use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

    assert!(matches!(
        crate::crypto::encrypt_encoded_content(Vec::new()).await,
        Err(XmtpError::InvalidInput(_))
    ));
    let without_type = EncodedContent {
        content: b"content".to_vec(),
        ..Default::default()
    };
    assert!(matches!(
        crate::crypto::encrypt_encoded_content(without_type.encode_to_vec()).await,
        Err(XmtpError::InvalidInput(_))
    ));
    let mut empty_authority = xmtp_content_types::text::TextCodec::encode("content".into())?;
    empty_authority
        .r#type
        .as_mut()
        .expect("content type")
        .authority_id
        .clear();
    assert!(matches!(
        crate::crypto::encrypt_encoded_content(empty_authority.encode_to_vec()).await,
        Err(XmtpError::InvalidInput(_))
    ));
    let mut empty_type = xmtp_content_types::text::TextCodec::encode("content".into())?;
    empty_type
        .r#type
        .as_mut()
        .expect("content type")
        .type_id
        .clear();
    assert!(matches!(
        crate::crypto::encrypt_encoded_content(empty_type.encode_to_vec()).await,
        Err(XmtpError::InvalidInput(_))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
fn standard_content_decodes_text() {
    use prost::Message as _;
    use xmtp_content_types::{ContentCodec, text::TextCodec};
    let encoded = TextCodec::encode("hello".into())?.encode_to_vec();
    assert!(
        matches!(crate::MessageContent::decode(encoded)?, crate::MessageContent::Text(value) if value == "hello")
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn client_configuration_and_credential_update() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let configured = client.server_configuration();
    let fetched =
        crate::client_identity::fetch_server_configuration(options().backend.unwrap()).await?;
    assert_eq!(configured.identifier, fetched.identifier);
    let refreshed = client.refresh_server_configuration().await?;
    assert_eq!(refreshed.identifier, configured.identifier);
    client.end().await?;
    let mut authenticated_options = options();
    let Some(BackendSource::Options {
        options: backend_options,
    }) = &mut authenticated_options.backend
    else {
        panic!("test uses backend options");
    };
    backend_options.credential = Some(Credential {
        name: None,
        value: "Bearer first".into(),
        expires_at_seconds: i64::MAX,
    });
    let authenticated =
        Client::create(crate::generate_local_signer().await, authenticated_options).await?;
    authenticated
        .set_credential(Credential {
            name: None,
            value: "Bearer test".into(),
            expires_at_seconds: i64::MAX,
        })
        .await?;
    assert!(matches!(
        authenticated
            .set_credential(Credential {
                name: Some("not a header".into()),
                value: "a".into(),
                expires_at_seconds: 0,
            })
            .await,
        Err(XmtpError::InvalidInput(_))
    ));
    authenticated.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn credential_can_be_set_after_build_without_initial_source() {
    use prost::bytes::Bytes;
    use xmtp_proto::api::{ApiClientError, BytesStream, Client as TransportClient};
    use xmtp_proto::api_client::XmtpBackendClient;

    struct CredentialProbe(Arc<AtomicBool>);

    #[xmtp_common::async_trait]
    impl TransportClient for CredentialProbe {
        fn host(&self) -> &str {
            "mock://credential-probe"
        }

        async fn request(
            &self,
            request: http::request::Builder,
            _path: http::uri::PathAndQuery,
            body: Bytes,
        ) -> Result<http::Response<Bytes>, ApiClientError> {
            assert_eq!(
                request
                    .headers_ref()
                    .and_then(|headers| headers.get(http::header::AUTHORIZATION)),
                Some(&http::header::HeaderValue::from_static(
                    "Bearer added-later"
                ))
            );
            self.0.store(true, Ordering::SeqCst);
            Ok(http::Response::new(body))
        }

        async fn stream(
            &self,
            _request: http::request::Builder,
            _path: http::uri::PathAndQuery,
            _body: Bytes,
        ) -> Result<http::Response<BytesStream>, ApiClientError> {
            unreachable!("credential proof uses a unary request")
        }
    }

    let backend = crate::Backend::from_options(BackendOptions {
        url: xmtp_configuration::backend_test_url(),
        ..Default::default()
    })?;
    assert!(!backend.api.has_credential_source());
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    client
        .set_credential(Credential {
            name: None,
            value: "Bearer added-later".into(),
            expires_at_seconds: i64::MAX,
        })
        .await?;
    let sent = Arc::new(AtomicBool::new(false));
    let middleware = xmtp_api_backend::AuthMiddleware::new(
        CredentialProbe(sent.clone()),
        None,
        client.auth_handle.clone(),
    );
    middleware
        .request(
            http::Request::builder(),
            http::uri::PathAndQuery::from_static("/credential-proof"),
            Bytes::new(),
        )
        .await?;
    assert!(sent.load(Ordering::SeqCst));
    client.end().await?;
}

#[xmtp_common::test]
fn client_options_backend_default_keeps_empty_connection_options() {
    let client_options = ClientOptions::default();
    assert!(client_options.backend.is_none());
    let BackendSource::Options { options } = client_options.backend.unwrap_or_default() else {
        panic!("default backend must use connection options");
    };
    assert_eq!(options.url, "");
}

#[xmtp_common::test(unwrap_try = true)]
async fn invalid_notification_key_has_typed_error() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let result = client
        .enable_notifications(crate::NotificationConfig {
            channel: crate::NotificationChannel::Http {
                url: "https://example.com".into(),
                signing_key: vec![1],
            },
            consent_states: None,
            include_welcomes: None,
            include_sync_groups: None,
            include_commits: None,
        })
        .await;
    assert!(matches!(result, Err(XmtpError::InvalidArgument(_))));
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn disabled_task_runner_has_typed_notification_error() {
    let mut settings = options();
    settings.workers = Some(crate::client::WorkerOptions {
        default_interval_ns: None,
        intervals: vec![crate::client::WorkerInterval {
            kind: crate::client::WorkerKind::TaskRunner,
            interval_ns: None,
            jitter_ns: None,
            enabled: Some(false),
        }],
    });
    let client = Client::create(crate::generate_local_signer().await, settings).await?;
    let result = client
        .enable_notifications(crate::NotificationConfig {
            channel: crate::NotificationChannel::Http {
                url: "https://example.com".into(),
                signing_key: vec![1; 16],
            },
            consent_states: None,
            include_welcomes: None,
            include_sync_groups: None,
            include_commits: None,
        })
        .await;
    assert!(matches!(result, Err(XmtpError::TaskRunnerDisabled(_))));
    client.end().await?;
}

#[xmtp_common::test]
fn notification_and_auth_errors_keep_their_kinds() {
    use crate::ErrorCategory;
    use xmtp_mls::client::notifications::NotificationError as N;
    use xmtp_proto::api::AuthError as A;

    macro_rules! notification {
        ($source:expr, $variant:ident, $category:pat, $retryable:expr) => {{
            let mapped = XmtpError::from_notification($source);
            let XmtpError::$variant(details) = mapped else {
                panic!("notification error became {mapped:?}");
            };
            assert_eq!(details.code, stringify!($variant));
            assert!(matches!(details.category, $category));
            assert_eq!(details.retryable, $retryable);
        }};
    }
    notification!(
        N::TaskRunnerDisabled,
        TaskRunnerDisabled,
        ErrorCategory::Notification,
        false
    );
    notification!(
        N::PermissionDenied,
        PermissionDenied,
        ErrorCategory::Notification,
        false
    );
    notification!(
        N::InvalidArgument,
        InvalidArgument,
        ErrorCategory::Notification,
        false
    );
    notification!(
        N::OutOfRange,
        OutOfRange,
        ErrorCategory::Notification,
        false
    );
    notification!(
        N::Unimplemented,
        Unimplemented,
        ErrorCategory::Notification,
        false
    );
    notification!(
        N::ChannelNotConfigured,
        ChannelNotConfigured,
        ErrorCategory::Notification,
        false
    );
    notification!(
        N::ResourceExhausted,
        ResourceExhausted,
        ErrorCategory::Notification,
        false
    );
    notification!(
        N::RequestTimeout,
        RequestTimeout,
        ErrorCategory::Notification,
        true
    );
    notification!(
        N::NotFound,
        NotificationNotFound,
        ErrorCategory::Notification,
        true
    );
    notification!(
        N::Api(xmtp_api::ApiError::InvalidRequest("test")),
        NotificationApi,
        ErrorCategory::Notification,
        false
    );
    notification!(
        N::Storage(xmtp_db::StorageError::DbDeserialize),
        NotificationStorage,
        ErrorCategory::Storage,
        false
    );
    notification!(
        N::Group(xmtp_mls::groups::GroupError::UserLimitExceeded),
        NotificationGroup,
        ErrorCategory::Conversation,
        false
    );

    let auth_cases = [
        (
            A::CredentialRejected { retryable: true },
            "CredentialRejected",
            true,
        ),
        (
            A::CallbackFailed { retryable: true },
            "CredentialCallbackFailed",
            true,
        ),
        (A::Exhausted, "CredentialExhausted", false),
        (A::ExhaustedAfterAttempt, "CredentialExhausted", false),
        (A::MissingCredential, "CredentialMissing", false),
    ];
    for (source, code, retryable) in auth_cases {
        let mapped = XmtpError::from_api(xmtp_api::ApiError::Auth(source));
        let details = match mapped {
            XmtpError::CredentialRejected(details)
            | XmtpError::CredentialCallbackFailed(details)
            | XmtpError::CredentialExhausted(details)
            | XmtpError::CredentialMissing(details) => details,
            other => panic!("auth error became {other:?}"),
        };
        assert_eq!(details.code, code);
        assert!(matches!(details.category, ErrorCategory::Callback));
        assert_eq!(details.retryable, retryable);
    }
    let nested = xmtp_mls::builder::ClientBuilderError::Identity(
        xmtp_mls::identity::IdentityError::ApiClient(xmtp_api::ApiError::Auth(A::CallbackFailed {
            retryable: true,
        })),
    );
    assert!(matches!(
        XmtpError::from_builder(nested),
        XmtpError::CredentialCallbackFailed(details) if details.retryable
    ));
}

#[xmtp_common::test(unwrap_try = true)]
fn out_of_range_installation_time_does_not_fail_inbox_state() {
    use xmtp_id::associations::{AssociationState, Identifier, Member, MemberIdentifier};
    let owner = Identifier::eth("0x1111111111111111111111111111111111111111")?;
    let installation = MemberIdentifier::installation(vec![1; 32]);
    let state = AssociationState::new(owner, 0, None)?.add(Member::new(
        installation,
        None,
        Some(u64::MAX),
        None,
    ));
    let state = crate::InboxState::from_core(state, None)?;
    assert_eq!(state.installations.len(), 1);
    assert_eq!(
        state.installations[0].created_at_ns,
        Some(crate::Timestamp(i64::MAX))
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn backend_only_identity_and_message_queries() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let Some(BackendSource::Options {
        options: backend_options,
    }) = options().backend
    else {
        panic!("test uses backend options");
    };
    let backend = Arc::new(crate::Backend::connect(backend_options).await?);
    let source = BackendSource::Connected { backend };
    let identity = client.identity();
    let inbox =
        crate::static_helpers::inbox_id_for_with_backend(source.clone(), identity.clone()).await?;
    assert_eq!(inbox, client.inbox_id());
    let availability =
        crate::static_helpers::can_message_with_backend(source.clone(), vec![identity.clone()])
            .await?;
    assert!(availability[0].can_message);
    let states =
        crate::static_helpers::inbox_states_with_backend(source.clone(), vec![inbox.clone()])
            .await?;
    assert_eq!(states[0].inbox_id, inbox);
    assert!(
        crate::static_helpers::is_address_authorized_with_backend(
            source.clone(),
            inbox.clone(),
            identity.identifier,
        )
        .await?
    );
    assert!(
        crate::static_helpers::is_installation_authorized_with_backend(
            source.clone(),
            inbox,
            client.installation_id(),
        )
        .await?
    );
    let group = client.conversations().create_group(vec![]).await?;
    group.send_text("metadata".into()).await?;
    let metadata = crate::static_helpers::newest_message_metadata_with_backend(
        source.clone(),
        vec![group.id()],
    )
    .await?;
    assert_eq!(metadata.len(), 1);
    let connected_client = Client::build(
        client.identity(),
        ClientOptions {
            backend: Some(source),
            ..options()
        },
        Some(client.inbox_id()),
    )
    .await?;
    assert_eq!(connected_client.inbox_id(), client.inbox_id());
    connected_client.end().await?;
    client.end().await?;
}

struct WalletSigner(PrivateKeySigner);

struct UnlistedChainSigner(PrivateKeySigner);

struct KindFailsSigner(PrivateKeySigner);

#[xmtp_common::async_trait]
impl Signer for KindFailsSigner {
    async fn identity(&self) -> Result<PublicIdentity, SignerError> {
        WalletSigner(self.0.clone()).identity().await
    }

    async fn kind(&self) -> Result<SignerKind, SignerError> {
        Err(SignerError::Failed)
    }

    async fn sign(&self, _request: SigningRequest) -> Result<Signature, SignerError> {
        Err(SignerError::Failed)
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn create_without_auto_registration_skips_signer_kind() {
    let mut settings = options();
    settings.registration.auto = false;
    let client = Client::create(
        Arc::new(KindFailsSigner(PrivateKeySigner::random())),
        settings,
    )
    .await?;
    client.end().await?;
}

#[xmtp_common::async_trait]
impl Signer for UnlistedChainSigner {
    async fn identity(&self) -> Result<PublicIdentity, SignerError> {
        WalletSigner(self.0.clone()).identity().await
    }

    async fn kind(&self) -> Result<SignerKind, SignerError> {
        Ok(SignerKind::Scw {
            chain_id: u64::MAX,
            block_number: None,
        })
    }

    async fn sign(&self, _request: SigningRequest) -> Result<Signature, SignerError> {
        Ok(Signature::Scw {
            bytes: vec![0; 65],
            address: self
                .0
                .get_identifier()
                .map_err(|_| SignerError::Failed)?
                .to_string(),
            chain_id: u64::MAX,
            block_number: None,
        })
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn static_revoke_rejects_unlisted_scw_chain() {
    let wallet = PrivateKeySigner::random();
    let client = Client::create(Arc::new(WalletSigner(wallet.clone())), options()).await?;
    let backend = options().backend.expect("backend");
    let result = crate::static_helpers::revoke_installations_with_backend(
        backend,
        Arc::new(UnlistedChainSigner(wallet)),
        client.inbox_id(),
        vec![client.installation_id()],
    )
    .await;
    assert!(
        matches!(result, Err(XmtpError::ChainNotAccepted(_))),
        "{result:?}"
    );
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn create_rejects_unlisted_scw_chain_with_typed_error() {
    let result = Client::create(
        Arc::new(UnlistedChainSigner(PrivateKeySigner::random())),
        options(),
    )
    .await;
    assert!(matches!(result, Err(XmtpError::ChainNotAccepted(_))));
}

#[xmtp_common::async_trait]
impl Signer for WalletSigner {
    async fn identity(&self) -> Result<PublicIdentity, SignerError> {
        Ok(PublicIdentity {
            identifier: self
                .0
                .get_identifier()
                .map_err(|_| SignerError::Failed)?
                .to_string(),
            kind: PublicIdentityKind::Ethereum,
        })
    }

    async fn kind(&self) -> Result<SignerKind, SignerError> {
        Ok(SignerKind::Eoa)
    }

    async fn sign(&self, request: SigningRequest) -> Result<Signature, SignerError> {
        let UnverifiedSignature::RecoverableEcdsa(signature) = self
            .0
            .sign(&request.text)
            .map_err(|_| SignerError::Failed)?
        else {
            return Err(SignerError::Failed);
        };
        Ok(Signature::Ecdsa(signature.signature_bytes().to_vec()))
    }
}

fn options() -> ClientOptions {
    ClientOptions {
        backend: Some(BackendSource::Options {
            options: BackendOptions {
                url: xmtp_configuration::backend_test_url(),
                app_version: None,
                credentials: None,
                credential: None,
            },
        }),
        storage: StorageOptions {
            location: StorageLocation::InMemory,
            label: None,
            encryption_key: None,
            pool: None,
            single_connection: false,
        },
        device_sync: false,
        ..ClientOptions::default()
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn slice_create_send_read_stream_end() {
    let alix = Arc::new(
        Client::create(
            Arc::new(WalletSigner(PrivateKeySigner::random())),
            options(),
        )
        .await?,
    );
    let bo = Arc::new(
        Client::create(
            Arc::new(WalletSigner(PrivateKeySigner::random())),
            options(),
        )
        .await?,
    );
    let group = alix
        .conversations()
        .create_group(vec![bo.inbox_id()])
        .await?;
    bo.inner.sync_welcomes().await?;
    let bo_group = crate::Group {
        inner: bo.inner.group(&group.inner.group_id)?,
        client_key: bo.key,
    };
    let id = group.send_text("hello from the slice".into()).await?;
    let history = group.messages().await?;
    let sent = history
        .into_iter()
        .find(|message| message.0.id == id)
        .expect("sent message");
    assert_eq!(sent.0.sender_inbox_id, alix.inbox_id());
    assert_eq!(sent.0.client_key, alix.key);
    assert!(sent.0.sent_at.0 > 0);
    assert!(
        matches!(sent.0.content, MessageContent::Text(ref text) if text == "hello from the slice")
    );

    let reader = bo_group.message_reader().await?;
    let received = xmtp_common::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            if let Some(message) = reader.next().await?
                && message.0.id == id
            {
                break Ok::<_, crate::XmtpError>(message);
            }
        }
    })
    .await??;
    assert_eq!(received.0.client_key, bo.key);
    assert_eq!(received.0.sender_inbox_id, alix.inbox_id());
    reader.end().await?;
    reader.end().await?;
    alix.end().await?;
    alix.end().await?;
    bo.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn idle_read_cancel_settles() {
    let client = Arc::new(
        Client::create(
            Arc::new(WalletSigner(PrivateKeySigner::random())),
            options(),
        )
        .await?,
    );
    let group = client.conversations().create_group(vec![]).await?;
    let reader = group.message_reader().await?;
    let mut cancelled = false;
    for _ in 0..32 {
        let pending =
            xmtp_common::time::timeout(std::time::Duration::from_millis(100), reader.next()).await;
        if pending.is_err() {
            cancelled = true;
            break;
        }
    }
    assert!(cancelled, "reader did not reach an idle read");
    reader.end().await?;
    assert!(reader.next().await?.is_none());
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn storage_default_requires_host_and_directory_names_are_unique() {
    let default = StorageOptions::default();
    assert!(matches!(
        native_storage_path(&default, "inbox-a"),
        Err(XmtpError::StorageLocationRequired(_))
    ));
    let built = Client::build(
        PublicIdentity {
            identifier: "invalid".into(),
            kind: PublicIdentityKind::Ethereum,
        },
        ClientOptions::default(),
        None,
    )
    .await;
    assert!(matches!(built, Err(XmtpError::StorageLocationRequired(_))));

    let directory = std::env::temp_dir().join(format!(
        "xmtp-sdk-storage-{}-{}",
        std::process::id(),
        xmtp_common::time::now_ns()
    ));
    let options = StorageOptions {
        location: StorageLocation::Directory(directory.to_string_lossy().into_owned()),
        label: None,
        encryption_key: None,
        pool: None,
        single_connection: false,
    };
    let first_path = native_storage_path(&options, "inbox-a")?.expect("directory path");
    let second_path = native_storage_path(&options, "inbox-b")?.expect("directory path");
    assert_ne!(first_path, second_path);
    assert!(first_path.ends_with("xmtp-inbox-a.db3"));
    assert!(second_path.ends_with("xmtp-inbox-b.db3"));
    let first_store = crate::client::open_store(&options, "inbox-a").await?;
    let second_store = crate::client::open_store(&options, "inbox-b").await?;
    assert!(std::path::Path::new(&first_path).exists());
    assert!(std::path::Path::new(&second_path).exists());
    let labeled = StorageOptions {
        label: Some("phone".into()),
        ..options.clone()
    };
    let labeled_path = native_storage_path(&labeled, "inbox-a")?.expect("directory path");
    assert!(labeled_path.ends_with("xmtp-phone-inbox-a.db3"));
    assert_ne!(first_path, labeled_path);
    let exact_path = directory
        .join("chosen.sqlite")
        .to_string_lossy()
        .into_owned();
    let path_options = StorageOptions {
        location: StorageLocation::Path(exact_path.clone()),
        ..options.clone()
    };
    assert_eq!(
        native_storage_path(&path_options, "inbox-a")?.expect("exact path"),
        exact_path
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&directory)?.permissions().mode() & 0o777,
            0o700
        );
    }
    drop(first_store);
    drop(second_store);
    std::fs::remove_dir_all(directory)?;
}

#[xmtp_common::test(unwrap_try = true)]
fn wasm_directory_reports_the_store_path() {
    let options = StorageOptions {
        location: StorageLocation::Directory("sdk-files".into()),
        label: Some("phone".into()),
        ..Default::default()
    };
    let reported = crate::client::wasm_storage_path(&options, "inbox-a")?.expect("file path");
    let location = crate::client::wasm_store_location(&options, "inbox-a")?;
    let xmtp_db::StorageOption::Persistent(opened) = &location else {
        panic!("Directory storage must be persistent");
    };
    assert_eq!(reported, opened.as_str());
    assert_eq!(reported, "sdk-files/xmtp-phone-inbox-a.db3");
}

#[xmtp_common::test(unwrap_try = true)]
async fn associated_wallet_uses_existing_inbox() {
    let wallet_a = PrivateKeySigner::random();
    let wallet_b = PrivateKeySigner::random();
    let client_a = Client::create(Arc::new(WalletSigner(wallet_a)), options()).await?;
    let mut request = client_a
        .inner
        .identity_updates()
        .associate_identity(wallet_b.get_identifier()?)
        .await?;
    let UnverifiedSignature::RecoverableEcdsa(signature) =
        wallet_b.sign(&request.signature_text())?
    else {
        panic!("wallet returned a non-ECDSA signature");
    };
    request
        .add_signature(
            UnverifiedSignature::new_recoverable_ecdsa(signature.signature_bytes().to_vec()),
            &client_a.inner.scw_verifier(),
        )
        .await?;
    client_a
        .inner
        .identity_updates()
        .apply_signature_request(request)
        .await?;
    let client_b = Client::create(Arc::new(WalletSigner(wallet_b)), options()).await?;
    assert_eq!(client_b.inbox_id(), client_a.inbox_id());
    client_b.end().await?;
    client_a.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn group_actions_return_client_closed_after_end() {
    let client = Client::create(
        Arc::new(WalletSigner(PrivateKeySigner::random())),
        options(),
    )
    .await?;
    let group = client.conversations().create_group(vec![]).await?;
    client.end().await?;
    assert!(matches!(
        group.send_text("after end".into()).await,
        Err(XmtpError::ClientClosed(_))
    ));
    assert!(matches!(
        group.messages().await,
        Err(XmtpError::ClientClosed(_))
    ));
    assert!(matches!(
        group.message_reader().await,
        Err(XmtpError::ClientClosed(_))
    ));
}

// `end()` cancels the context first and disconnects the database last. Stop
// after the first step: an operation that races `end()` sees this state, and
// the test can still read what the operation wrote.
fn begin_end(client: &Client) {
    client.inner.context.cancellation_token().cancel();
}

#[xmtp_common::test(unwrap_try = true)]
async fn create_group_racing_end_is_closed_and_persists_nothing() {
    let client = Client::create(
        Arc::new(WalletSigner(PrivateKeySigner::random())),
        options(),
    )
    .await?;
    let before = client.inner.find_groups(GroupQueryArgs::default())?.len();
    begin_end(&client);
    assert!(matches!(
        client.conversations().create_group(vec![]).await,
        Err(XmtpError::ClientClosed(_))
    ));
    assert_eq!(
        client.inner.find_groups(GroupQueryArgs::default())?.len(),
        before
    );
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn send_text_racing_end_is_closed_and_persists_nothing() {
    let client = Client::create(
        Arc::new(WalletSigner(PrivateKeySigner::random())),
        options(),
    )
    .await?;
    let group = client.conversations().create_group(vec![]).await?;
    let before = group.inner.find_messages(&MsgQueryArgs::default())?.len();
    begin_end(&client);
    assert!(matches!(
        group.send_text("racing end".into()).await,
        Err(XmtpError::ClientClosed(_))
    ));
    assert_eq!(
        group.inner.find_messages(&MsgQueryArgs::default())?.len(),
        before
    );
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn message_reader_racing_end_is_closed_and_takes_no_lease() {
    let client = Client::create(
        Arc::new(WalletSigner(PrivateKeySigner::random())),
        options(),
    )
    .await?;
    let group = client.conversations().create_group(vec![]).await?;
    begin_end(&client);
    assert!(matches!(
        group.message_reader().await,
        Err(XmtpError::ClientClosed(_))
    ));
    assert!(client.inner.context.delivery_owner().lock().is_none());
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn reader_end_rejects_pending_handoff() {
    let client = Client::create(
        Arc::new(WalletSigner(PrivateKeySigner::random())),
        options(),
    )
    .await?;
    let group = client.conversations().create_group(vec![]).await?;
    let reader = group.message_reader().await?;
    let gate = Arc::new(reader::HandoffGate {
        arrived: Notify::new(),
        release: Notify::new(),
    });
    *reader.handoff_gate.lock() = Some(gate.clone());
    group.send_text("pending".into()).await?;
    let pending_reader = reader.clone();
    let pending = tokio::spawn(async move { pending_reader.next().await });
    xmtp_common::time::timeout(Duration::from_secs(10), gate.arrived.notified()).await?;
    let ending_reader = reader.clone();
    let ending = tokio::spawn(async move { ending_reader.end().await });
    xmtp_common::time::timeout(Duration::from_secs(10), async {
        while !reader.is_ended_for_test() {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    gate.release.notify_one();
    assert!(pending.await??.is_none());
    ending.await??;
    assert!(reader.next().await?.is_none());
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn reader_selection_changed_is_not_a_fatal_error() {
    assert!(reader::selection_changed(
        &LocalDeliveryError::SelectionChanged
    ));
    assert!(!reader::selection_changed(&LocalDeliveryError::Closed));
}

#[xmtp_common::test(unwrap_try = true)]
async fn reader_skips_handoff_removed_from_scope() {
    let client = Client::create(
        Arc::new(WalletSigner(PrivateKeySigner::random())),
        options(),
    )
    .await?;
    let stale_group = client.conversations().create_group(vec![]).await?;
    let live_group = client.conversations().create_group(vec![]).await?;
    let reader = stale_group.message_reader().await?;
    let gate = Arc::new(reader::HandoffGate {
        arrived: Notify::new(),
        release: Notify::new(),
    });
    *reader.handoff_gate.lock() = Some(gate.clone());
    stale_group.send_text("stale".into()).await?;
    live_group.send_text("live".into()).await?;

    let pending_reader = reader.clone();
    let pending = tokio::spawn(async move { pending_reader.next().await });
    xmtp_common::time::timeout(Duration::from_secs(10), gate.arrived.notified()).await?;
    reader.update_scope_for_test(vec![live_group.inner.group_id]);
    gate.release.notify_one();

    let delivered = xmtp_common::time::timeout(Duration::from_secs(10), pending).await???;
    let delivered = delivered.expect("reader must continue after rejecting the stale item");
    assert_eq!(delivered.0.conversation_id, live_group.id());
    assert_ne!(delivered.0.conversation_id, stale_group.id());
    reader.end().await?;
    client.end().await?;
}

struct SlowSigner {
    started: Arc<Notify>,
    release: Arc<Notify>,
    completed: Arc<AtomicBool>,
    dropped_early: Arc<AtomicBool>,
}

struct CompletionGuard {
    completed: Arc<AtomicBool>,
    dropped_early: Arc<AtomicBool>,
}

impl Drop for CompletionGuard {
    fn drop(&mut self) {
        if !self.completed.load(Ordering::SeqCst) {
            self.dropped_early.store(true, Ordering::SeqCst);
        }
    }
}

#[xmtp_common::async_trait]
impl Signer for SlowSigner {
    async fn identity(&self) -> Result<PublicIdentity, SignerError> {
        Err(SignerError::Failed)
    }

    async fn kind(&self) -> Result<SignerKind, SignerError> {
        Ok(SignerKind::Eoa)
    }

    async fn sign(&self, _request: SigningRequest) -> Result<Signature, SignerError> {
        let _guard = CompletionGuard {
            completed: self.completed.clone(),
            dropped_early: self.dropped_early.clone(),
        };
        self.started.notify_one();
        self.release.notified().await;
        self.completed.store(true, Ordering::SeqCst);
        Ok(Signature::Ecdsa(vec![0; 65]))
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn foreign_call_not_dropped_on_cancel() {
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let completed = Arc::new(AtomicBool::new(false));
    let dropped_early = Arc::new(AtomicBool::new(false));
    let signer: Arc<dyn Signer> = Arc::new(SlowSigner {
        started: started.clone(),
        release: release.clone(),
        completed: completed.clone(),
        dropped_early: dropped_early.clone(),
    });
    let caller = tokio::spawn(async move {
        let _ = signer::sign(
            signer,
            SigningRequest {
                text: "test".into(),
            },
        )
        .await;
    });
    started.notified().await;
    caller.abort();
    let _ = caller.await;
    release.notify_one();
    xmtp_common::time::timeout(std::time::Duration::from_secs(5), async {
        while !completed.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert!(!dropped_early.load(Ordering::SeqCst));
}

struct BlockingProbe {
    started: parking_lot::Mutex<Option<std::sync::mpsc::Sender<()>>>,
    release: Arc<AtomicBool>,
    emergency: Arc<AtomicBool>,
}

impl BlockingProbe {
    fn wait(&self) {
        if let Some(started) = self.started.lock().take() {
            let _ = started.send(());
        }
        while !self.release.load(Ordering::SeqCst) && !self.emergency.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

struct BlockingSigner(Arc<BlockingProbe>);

#[xmtp_common::async_trait]
impl Signer for BlockingSigner {
    async fn identity(&self) -> Result<PublicIdentity, SignerError> {
        Err(SignerError::Failed)
    }

    async fn kind(&self) -> Result<SignerKind, SignerError> {
        Ok(SignerKind::Eoa)
    }

    async fn sign(&self, _request: SigningRequest) -> Result<Signature, SignerError> {
        self.0.wait();
        Ok(Signature::Ecdsa(vec![0; 65]))
    }
}

struct BlockingCredentials(Arc<BlockingProbe>);

#[xmtp_common::async_trait]
impl CredentialSource for BlockingCredentials {
    async fn credential(&self) -> Result<Credential, CredentialError> {
        self.0.wait();
        Ok(Credential {
            name: None,
            value: "token".into(),
            expires_at_seconds: 0,
        })
    }
}

async fn assert_foreign_call_off_executor<F, Fut>(call: F)
where
    F: FnOnce(Arc<BlockingProbe>) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("current-thread runtime");
        let (started, started_rx) = std::sync::mpsc::channel();
        let release = Arc::new(AtomicBool::new(false));
        let emergency = Arc::new(AtomicBool::new(false));
        let probe = Arc::new(BlockingProbe {
            started: parking_lot::Mutex::new(Some(started)),
            release: release.clone(),
            emergency: emergency.clone(),
        });
        let handle = runtime.handle().clone();
        let controller_emergency = emergency.clone();
        let controller = std::thread::spawn(move || {
            let started = started_rx.recv_timeout(Duration::from_secs(5)).is_ok();
            if started {
                let for_task = release.clone();
                handle.spawn(async move {
                    for_task.store(true, Ordering::SeqCst);
                });
                let deadline = std::time::Instant::now() + Duration::from_secs(2);
                while !release.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            if !release.load(Ordering::SeqCst) {
                controller_emergency.store(true, Ordering::SeqCst);
            }
            started
        });
        let result = runtime.block_on(tokio::time::timeout(Duration::from_secs(6), call(probe)));
        assert!(
            controller.join().expect("probe controller"),
            "foreign call did not start"
        );
        assert!(result.is_ok(), "foreign call did not complete");
        assert!(
            !emergency.load(Ordering::SeqCst),
            "foreign call blocked the executor thread"
        );
    })
    .await
    .expect("probe runtime thread");
}

#[xmtp_common::test(unwrap_try = true)]
async fn signer_and_credential_calls_start_off_executor() {
    assert_foreign_call_off_executor(|probe| async move {
        let signer: Arc<dyn Signer> = Arc::new(BlockingSigner(probe));
        signer::sign(
            signer,
            SigningRequest {
                text: "probe".into(),
            },
        )
        .await
        .expect("signer call");
    })
    .await;
    assert_foreign_call_off_executor(|probe| async move {
        let source: Arc<dyn CredentialSource> = Arc::new(BlockingCredentials(probe));
        let bridge = AuthBridge::new(source);
        xmtp_api_backend::AuthCallback::on_auth_required(&bridge)
            .await
            .expect("credential call");
    })
    .await;
}

#[xmtp_common::test(unwrap_try = true)]
async fn message_ids_round_trip_hex() {
    let raw = xmtp_proto::types::GroupId::from([0xab; 16]);
    let conversation = ConversationID::from(raw);
    assert_eq!(conversation.0, "ab".repeat(16));
    assert_eq!(xmtp_proto::types::GroupId::try_from(conversation)?, raw);
    let message = MessageID::from_bytes(&[0xcd; 32])?;
    assert_eq!(MessageID::try_from(message.0.clone())?, message);
    assert!(MessageID::try_from("CD".repeat(32)).is_err());
    assert!(ConversationID::try_from("ab".repeat(15)).is_err());
    assert!(InboxID::try_from(String::new()).is_err());
}

#[xmtp_common::test(unwrap_try = true)]
async fn callback_errors_convert() {
    fn assert_from<T: From<uniffi::UnexpectedUniFFICallbackError>>() {}
    assert_from::<SignerError>();
    assert_from::<CredentialError>();
}
