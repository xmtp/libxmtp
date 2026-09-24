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
    BackendOptions, Client, ClientOptions, ConversationID, Credential, CredentialError,
    CredentialSource, InboxID, MessageContent, MessageID, PublicIdentity, PublicIdentityKind,
    Signature, Signer, SignerError, SignerKind, SigningRequest, StorageLocation, StorageOptions,
    XmtpError, client::native_storage_path, credentials::AuthBridge, reader, signer,
};

struct WalletSigner(PrivateKeySigner);

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
        backend: BackendOptions {
            url: xmtp_configuration::backend_test_url(),
            app_version: None,
            credentials: None,
        },
        storage: StorageOptions {
            location: StorageLocation::InMemory,
            label: None,
            encryption_key: None,
        },
        device_sync: false,
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
