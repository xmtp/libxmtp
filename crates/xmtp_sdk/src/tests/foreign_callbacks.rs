use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn credential_callbacks_stay_serial_after_caller_abort() {
    use std::sync::atomic::AtomicUsize;
    use tokio::sync::{Semaphore, mpsc};
    use xmtp_api_backend::AuthCallback;

    struct HeldCredential {
        entered: mpsc::UnboundedSender<()>,
        release: Semaphore,
        active: AtomicUsize,
        maximum: AtomicUsize,
        calls: AtomicUsize,
    }

    #[xmtp_common::async_trait]
    impl CredentialSource for HeldCredential {
        async fn credential(&self) -> Result<Credential, CredentialError> {
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.maximum.fetch_max(active, Ordering::SeqCst);
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.entered.send(()).unwrap();
            self.release.acquire().await.unwrap().forget();
            self.active.fetch_sub(1, Ordering::SeqCst);
            Ok(Credential {
                name: None,
                value: "test-token".into(),
                expires_at_seconds: i64::MAX,
            })
        }
    }

    let (entered, mut entries) = mpsc::unbounded_channel();
    let source = Arc::new(HeldCredential {
        entered,
        release: Semaphore::new(0),
        active: AtomicUsize::new(0),
        maximum: AtomicUsize::new(0),
        calls: AtomicUsize::new(0),
    });
    let bridge = Arc::new(AuthBridge::new(source.clone()));
    let first_bridge = bridge.clone();
    let first = tokio::spawn(async move { first_bridge.on_auth_required().await });
    entries.recv().await.unwrap();
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());

    let mut cancelled_waiter = Box::pin(bridge.on_auth_required());
    assert!(futures::poll!(cancelled_waiter.as_mut()).is_pending());
    drop(cancelled_waiter);

    let second = tokio::spawn(async move { bridge.on_auth_required().await });
    let overlapped = xmtp_common::time::timeout(Duration::from_millis(100), entries.recv())
        .await
        .is_ok();
    source.release.add_permits(1);
    if !overlapped {
        xmtp_common::time::timeout(Duration::from_secs(5), entries.recv()).await?;
    }
    source.release.add_permits(3);
    xmtp_common::time::timeout(Duration::from_secs(5), second)
        .await??
        .unwrap();
    assert_eq!(source.maximum.load(Ordering::SeqCst), 1);
    assert_eq!(source.active.load(Ordering::SeqCst), 0);
    assert_eq!(source.calls.load(Ordering::SeqCst), 2);
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
    let conversation = ConversationId::from(raw);
    assert_eq!(conversation.checked()?, "ab".repeat(16));
    assert_eq!(xmtp_proto::types::GroupId::try_from(conversation)?, raw);
    let message = MessageId::from_bytes(&[0xcd; 32])?;
    assert_eq!(MessageId::try_from(message.checked()?.to_owned())?, message);
    assert!(matches!(
        MessageId::try_from("CD".repeat(32)),
        Err(XmtpError::InvalidArgument(_))
    ));
    assert!(matches!(
        ConversationId::try_from("ab".repeat(15)),
        Err(XmtpError::InvalidArgument(_))
    ));
    assert!(matches!(
        InboxId::try_from(String::new()),
        Err(XmtpError::InvalidArgument(_))
    ));
}

/// Lifts host text the way generated bindings do.
fn host_id<T>(text: &str) -> T
where
    T: uniffi::Lift<crate::UniFfiTag, FfiType = uniffi::RustBuffer>,
{
    T::try_lift(uniffi::RustBuffer::from_vec(text.as_bytes().to_vec()))
        .expect("host ID lift must not fail")
}

fn assert_invalid_argument<T>(result: Result<T, XmtpError>) {
    match result {
        Err(XmtpError::InvalidArgument(details)) => {
            assert_eq!(details.code, "InvalidArgument");
            assert!(matches!(details.category, crate::ErrorCategory::Input));
            assert!(!details.retryable);
        }
        Err(error) => panic!("expected InvalidArgument, got {error:?}"),
        Ok(_) => panic!("malformed ID was accepted"),
    }
}

// Uppercase hex decodes, so only ID validation rejects it.
fn uppercase_hex(bytes: usize) -> String {
    "AB".repeat(bytes)
}

#[xmtp_common::test(unwrap_try = true)]
async fn host_id_lift_defers_validation_to_checked_reads() {
    let message: MessageId = host_id(&uppercase_hex(32));
    assert_invalid_argument(message.checked());
    assert_invalid_argument(message.to_bytes());
    let installation: crate::InstallationId = host_id(&uppercase_hex(32));
    assert_invalid_argument(installation.checked());
    assert_invalid_argument(installation.to_bytes());
    let conversation: ConversationId = host_id(&"ab".repeat(15));
    assert_invalid_argument(conversation.to_bytes());
    assert_invalid_argument(xmtp_proto::types::GroupId::try_from(conversation));
    let inbox: InboxId = host_id("");
    assert_invalid_argument(inbox.clone().into_checked());

    let valid: MessageId = host_id(&"ab".repeat(32));
    assert_eq!(valid.checked()?, "ab".repeat(32));
    assert_eq!(valid.to_bytes()?, vec![0xab; 32]);

    let nested = crate::EncodedContent {
        r#type: crate::standard_content_type(crate::StandardContentKind::Text),
        parameters: Default::default(),
        fallback: None,
        content: b"hi".to_vec(),
    };
    let reaction = crate::Reaction {
        content: "+".into(),
        action: crate::ReactionAction::Added,
        schema: crate::ReactionSchema::Unicode,
    };
    let update = |added: InboxId| crate::GroupUpdated {
        initiated_by_inbox_id: host_id("inbox"),
        added_inboxes: vec![added],
        removed_inboxes: vec![],
        left_inboxes: vec![],
        metadata_field_changes: vec![],
        added_admin_inboxes: vec![],
        removed_admin_inboxes: vec![],
        added_super_admin_inboxes: vec![],
        removed_super_admin_inboxes: vec![],
    };
    for content in [
        crate::StandardContent::DeleteMessage {
            message_id: host_id(&uppercase_hex(32)),
        },
        crate::StandardContent::Reaction {
            reference: host_id(&uppercase_hex(32)),
            reference_inbox_id: None,
            reaction: reaction.clone(),
        },
        crate::StandardContent::Reaction {
            reference: host_id(&"ab".repeat(32)),
            reference_inbox_id: Some(host_id("")),
            reaction,
        },
        crate::StandardContent::Reply {
            reference: host_id(&uppercase_hex(32)),
            reference_inbox_id: None,
            content: nested.clone(),
        },
        crate::StandardContent::Reply {
            reference: host_id(&"ab".repeat(32)),
            reference_inbox_id: Some(host_id("")),
            content: nested,
        },
        crate::StandardContent::GroupUpdated(update(host_id(""))),
    ] {
        assert_invalid_argument(crate::encode_standard(content));
    }
    crate::encode_standard(crate::StandardContent::GroupUpdated(update(host_id(
        "peer",
    ))))?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn malformed_host_ids_fail_operations_with_invalid_argument() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let conversations = client.conversations();
    assert_invalid_argument(
        conversations
            .get_message_by_id(host_id(&uppercase_hex(32)))
            .await,
    );
    assert_invalid_argument(conversations.get_by_id(host_id(&uppercase_hex(16))).await);
    assert_invalid_argument(
        client
            .key_package_statuses(vec![host_id(&uppercase_hex(32))])
            .await,
    );
    assert_invalid_argument(client.inbox_states(vec![host_id("")], false).await);
    assert_invalid_argument(
        client
            .preferences()
            .consent_state(crate::ConsentEntity::Conversation {
                conversation_id: host_id(&uppercase_hex(16)),
            })
            .await,
    );
    client.end().await?;
}

// A closed client proves the IDs are checked before the open check and the
// stored-conversation lookup.
#[xmtp_common::test(unwrap_try = true)]
async fn malformed_event_filter_ids_fail_before_client_access() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let filter = || EventFilter {
        kinds: vec![EventKind::MessageReceived],
        group_ids: Some(vec![vec![0xab; 16], vec![]]),
        ..EventFilter::default()
    };
    client.end().await?;
    assert_invalid_argument(client.events(filter()).await);
    let (started, _) = tokio::sync::mpsc::unbounded_channel();
    let listener = Arc::new(EventProbe {
        started,
        completed: Arc::new(AtomicBool::new(false)),
        release: None,
        calls: Default::default(),
        active: Default::default(),
        maximum: Default::default(),
        fail_first: false,
        reenter: None,
        end_inside: false,
    });
    assert_invalid_argument(client.start_listener(filter(), listener).await);
}

struct IdentityCountingSigner {
    inner: Arc<dyn Signer>,
    identity_calls: std::sync::atomic::AtomicUsize,
}

#[xmtp_common::async_trait]
impl Signer for IdentityCountingSigner {
    async fn identity(&self) -> Result<PublicIdentity, SignerError> {
        self.identity_calls.fetch_add(1, Ordering::SeqCst);
        self.inner.identity().await
    }

    async fn kind(&self) -> Result<SignerKind, SignerError> {
        self.inner.kind().await
    }

    async fn sign(&self, request: SigningRequest) -> Result<Signature, SignerError> {
        self.inner.sign(request).await
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn malformed_fork_recovery_id_fails_before_signer_call() {
    let signer = Arc::new(IdentityCountingSigner {
        inner: crate::generate_local_signer().await,
        identity_calls: Default::default(),
    });
    let mut settings = options();
    settings.fork_recovery = Some(crate::client::ForkRecoveryOptions {
        groups: vec![host_id(&uppercase_hex(16))],
        ..Default::default()
    });
    assert_invalid_argument(Client::create(signer.clone(), settings).await);
    assert_eq!(signer.identity_calls.load(Ordering::SeqCst), 0);
}

#[xmtp_common::test(unwrap_try = true)]
async fn callback_errors_convert() {
    fn assert_from<T: From<uniffi::UnexpectedUniFFICallbackError>>() {}
    assert_from::<SignerError>();
    assert_from::<CredentialError>();
    assert_from::<crate::PreAuthenticateError>();
}

#[xmtp_common::test(unwrap_try = true)]
fn reaction_unknown_values_remain_unknown() {
    use xmtp_proto::xmtp::mls::message_contents::content_types::ReactionV2;

    let reaction = crate::Reaction::from_proto(ReactionV2 {
        action: 0,
        schema: i32::MAX,
        ..Default::default()
    });
    assert_eq!(format!("{:?}", reaction.action), "Unknown");
    assert_eq!(format!("{:?}", reaction.schema), "Unknown");
}
