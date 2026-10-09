#![cfg(not(target_arch = "wasm32"))]

mod history_pages;
mod history_snapshot;
mod reader_ack_cancellation;
mod reader_admission;
mod reader_cancellation_regressions;
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
    StorageLocation, StorageOptions, XmtpError, credentials::AuthBridge, reader, signer,
};

use crate::{ClientEvent, EventFilter, EventKind, EventListener, ListenerError};
use xmtp_events::{EventWriter, HmacKeysUpdated};

/// A database file with its attachments directory beside it.
fn explicit_location(path: &std::path::Path) -> StorageLocation {
    StorageLocation::Explicit {
        db_path: path.to_string_lossy().into_owned(),
        attachments_dir: path
            .with_extension("attachments")
            .to_string_lossy()
            .into_owned(),
    }
}

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

/// A directory name no other test run uses. The test creates it.
fn temp_root(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "xmtp-sdk-{name}-{}-{}",
        std::process::id(),
        xmtp_common::time::now_ns()
    ))
}

const ATTACHMENT_KINDS: [EventKind; 7] = [
    EventKind::AttachmentUploadStarted,
    EventKind::AttachmentUploadCompleted,
    EventKind::AttachmentUploadFailed,
    EventKind::AttachmentDownloadStarted,
    EventKind::AttachmentDownloadCompleted,
    EventKind::AttachmentDownloadFailed,
    EventKind::AttachmentDeleted,
];

fn core_attachment(key: &str) -> xmtp_events::AttachmentRef {
    xmtp_events::AttachmentRef {
        attachment_key: key.into(),
        url: format!("https://example.com/{key}"),
        content_digest: format!("digest-{key}"),
    }
}

fn core_attachment_failed(key: &str, cause: &str) -> xmtp_events::AttachmentFailed {
    xmtp_events::AttachmentFailed {
        attachment_key: key.into(),
        url: format!("https://example.com/{key}"),
        content_digest: format!("digest-{key}"),
        cause: cause.into(),
    }
}

/// Emit one event of each attachment kind: an upload of `up`, a failed
/// upload of `rejected`, a download and deletion of `down`, and a failed
/// download of `corrupt`.
fn emit_attachment_kinds(client: &Client) {
    use xmtp_events::ClientEvent as Core;
    for event in [
        Core::AttachmentUploadStarted(core_attachment("up")),
        Core::AttachmentUploadCompleted(core_attachment("up")),
        Core::AttachmentUploadFailed(core_attachment_failed("rejected", "backend_rejected")),
        Core::AttachmentDownloadStarted(core_attachment("down")),
        Core::AttachmentDownloadCompleted(core_attachment("down")),
        Core::AttachmentDownloadFailed(core_attachment_failed("corrupt", "digest_mismatch")),
        Core::AttachmentDeleted(core_attachment("down")),
    ] {
        client.inner.context.events().emit(Some(event), None);
    }
}

fn assert_attachment_kinds(events: &[ClientEvent]) {
    use crate::{AttachmentFailed, AttachmentRef};
    let reference = |key: &str| AttachmentRef {
        attachment_key: key.into(),
        url: format!("https://example.com/{key}"),
        content_digest: format!("digest-{key}"),
    };
    let failed = |key: &str, cause| AttachmentFailed {
        attachment_key: key.into(),
        url: format!("https://example.com/{key}"),
        content_digest: format!("digest-{key}"),
        cause,
    };
    let [
        ClientEvent::AttachmentUploadStarted {
            attachment_upload_started: started,
        },
        ClientEvent::AttachmentUploadCompleted {
            attachment_upload_completed: uploaded,
        },
        ClientEvent::AttachmentUploadFailed {
            attachment_upload_failed: upload_failed,
        },
        ClientEvent::AttachmentDownloadStarted {
            attachment_download_started: downloading,
        },
        ClientEvent::AttachmentDownloadCompleted {
            attachment_download_completed: downloaded,
        },
        ClientEvent::AttachmentDownloadFailed {
            attachment_download_failed: download_failed,
        },
        ClientEvent::AttachmentDeleted {
            attachment_deleted: deleted,
        },
    ] = events
    else {
        panic!("expected the seven attachment kinds in order, got {events:?}");
    };
    assert_eq!(started, &reference("up"));
    assert_eq!(uploaded, &reference("up"));
    assert_eq!(
        upload_failed,
        &failed("rejected", "backend_rejected".into())
    );
    assert_eq!(downloading, &reference("down"));
    assert_eq!(downloaded, &reference("down"));
    assert_eq!(
        download_failed,
        &failed("corrupt", "digest_mismatch".into())
    );
    assert_eq!(deleted, &reference("down"));
}

struct EventCapture(tokio::sync::mpsc::UnboundedSender<ClientEvent>);

#[xmtp_common::async_trait]
impl EventListener for EventCapture {
    async fn on_event(&self, event: ClientEvent) -> Result<(), ListenerError> {
        let _ = self.0.send(event);
        Ok(())
    }
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
    case: &str,
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
            let preserved = message.0.id == id && matches!(message.0.content,
                MessageContent::Unknown { encoded, raw_bytes, .. }
                    if raw_bytes.as_slice() == expected_raw
                        && encoded.as_ref().is_none_or(|value| value.r#type.authority_id == "xmtp.org" && value.r#type.type_id == "text"));
            (path, preserved)
        })
        .collect::<Vec<_>>();
    assert!(
        outcomes.iter().all(|(_, preserved)| *preserved),
        "{case}: failed standard content was not Unknown with raw bytes: {outcomes:?}"
    );
    Ok(())
}

mod archives;
mod attachment_flows;
mod backend_queries;
mod callback_lifetime;
mod client_build;
mod client_setup;
mod connections;
mod content_decode;
mod content_filters;
mod content_records;
mod content_validation;
mod create_adoption;
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
mod metadata_fields;
mod permissions;
mod public_error_actions;
mod query_costs;
mod reader_delivery;
mod reader_recovery;
mod retained_content;
mod signers;
mod standard_sends;
mod storage;
mod storage_layout;
mod storage_retry;
mod transparent_wrappers;

mod reader_cursor;

mod by_id_privacy;
