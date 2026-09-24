#![cfg(not(target_arch = "wasm32"))]

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use alloy::signers::local::PrivateKeySigner;
use tokio::sync::Notify;
use xmtp_id::{InboxOwner, associations::unverified::UnverifiedSignature};

use crate::{
    BackendOptions, Client, ClientOptions, ConversationID, CredentialError, InboxID,
    MessageContent, MessageID, PublicIdentity, PublicIdentityKind, Signature, Signer, SignerError,
    SignerKind, SigningRequest, StorageLocation, StorageOptions, signer,
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
