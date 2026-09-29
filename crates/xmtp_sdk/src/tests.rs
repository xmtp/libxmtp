#![cfg(not(target_arch = "wasm32"))]

mod reader_admission;
mod reader_restored;
mod reader_selection;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::{future::Future, time::Duration};

use alloy::signers::local::PrivateKeySigner;
use futures::FutureExt;
use tokio::sync::Notify;
use xmtp_db::{group::GroupQueryArgs, group_message::MsgQueryArgs};
use xmtp_id::{InboxOwner, associations::unverified::UnverifiedSignature};
use xmtp_mls::context::XmtpSharedContext;
use xmtp_mls::subscriptions::local_delivery::LocalDeliveryError;

use crate::{
    BackendOptions, BackendSource, Client, ClientOptions, ConversationId, Credential,
    CredentialError, CredentialSource, InboxId, MessageContent, MessageId, PublicIdentity,
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

/// Signs as a local key, but holds its signer kind until the test releases it.
struct PendingKindSigner {
    inner: Arc<dyn Signer>,
    kind_started: Arc<Notify>,
    kind_release: Arc<Notify>,
}

#[xmtp_common::async_trait]
impl Signer for PendingKindSigner {
    async fn identity(&self) -> Result<PublicIdentity, SignerError> {
        self.inner.identity().await
    }

    async fn kind(&self) -> Result<SignerKind, SignerError> {
        self.kind_started.notify_one();
        self.kind_release.notified().await;
        Ok(SignerKind::Eoa)
    }

    async fn sign(&self, request: SigningRequest) -> Result<Signature, SignerError> {
        self.inner.sign(request).await
    }
}

mod binding_map;

struct RecordingPreAuthenticate {
    calls: Arc<std::sync::Mutex<Vec<&'static str>>>,
    fail: bool,
}

#[xmtp_common::async_trait]
impl crate::PreAuthenticate for RecordingPreAuthenticate {
    async fn run(&self) -> Result<(), crate::PreAuthenticateError> {
        self.calls.lock().expect("calls").push("pre-authenticate");
        if self.fail {
            Err(crate::PreAuthenticateError::Failed)
        } else {
            Ok(())
        }
    }
}

struct RecordingSigner {
    key: PrivateKeySigner,
    calls: Arc<std::sync::Mutex<Vec<&'static str>>>,
}

#[xmtp_common::async_trait]
impl Signer for RecordingSigner {
    async fn identity(&self) -> Result<PublicIdentity, SignerError> {
        WalletSigner(self.key.clone()).identity().await
    }

    async fn kind(&self) -> Result<SignerKind, SignerError> {
        Ok(SignerKind::Eoa)
    }

    async fn sign(&self, request: SigningRequest) -> Result<Signature, SignerError> {
        self.calls.lock().expect("calls").push("sign");
        WalletSigner(self.key.clone()).sign(request).await
    }
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

async fn wait_for_initial_connection<F, Fut>(
    mut changed: F,
) -> Result<crate::ConnectionState, XmtpError>
where
    F: FnMut(crate::ConnectionState) -> Fut,
    Fut: Future<Output = Result<crate::ConnectionState, XmtpError>>,
{
    use crate::ConnectionState;

    let mut previous = ConnectionState::Connecting;
    loop {
        let current = changed(previous).await?;
        match current {
            ConnectionState::Connected => return Ok(current),
            ConnectionState::Connecting | ConnectionState::Reconnecting => previous = current,
            ConnectionState::Failed | ConnectionState::Closed => {
                panic!("initial connection stopped at {current:?}")
            }
        }
    }
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

async fn assert_undecodable_standard_read_paths(
    client: &Client,
    group: &Arc<crate::Group>,
    id: MessageId,
    expected_raw: &[u8],
) -> Result<(), XmtpError> {
    let stored = client
        .inner
        .message(id.to_bytes()?)
        .map_err(XmtpError::unknown)?;
    let direct = crate::Message::from_stored(stored, client.client_key())?;
    let by_id = client
        .conversations()
        .get_message_by_id(id.clone())
        .await?
        .expect("message by ID");
    let history = group
        .messages(None)
        .await?
        .into_iter()
        .find(|message| message.0.id == id)
        .expect("message in history");
    let outcomes = [("direct", direct), ("by ID", by_id), ("history", history)]
        .into_iter()
        .map(|(path, message)| {
            let preserved = matches!(message.0.content,
                MessageContent::Unknown { encoded, raw_bytes }
                    if raw_bytes.as_slice() == expected_raw
                        && encoded.r#type.authority_id == "xmtp.org"
                        && encoded.r#type.type_id == "text");
            (path, preserved)
        })
        .collect::<Vec<_>>();
    assert!(
        outcomes.iter().all(|(_, preserved)| *preserved),
        "failed standard content was not Unknown with raw bytes: {outcomes:?}"
    );
    Ok(())
}

mod archives;
mod backend_queries;
mod client_setup;
mod connections;
mod content_decode;
mod content_filters;
mod content_records;
mod content_validation;
mod create_cleanup;
mod error_records;
mod event_listeners;
mod event_readers;
mod foreign_callbacks;
mod group_options;
mod history_errors;
mod identity_routes;
mod lifecycle;
mod message_actions;
mod permissions;
mod query_costs;
mod reader_delivery;
mod reader_recovery;
mod signers;
mod standard_sends;
mod storage;

mod reader_cursor;
