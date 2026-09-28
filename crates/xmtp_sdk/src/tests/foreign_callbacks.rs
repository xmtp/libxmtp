use super::*;

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
    assert_eq!(conversation.0, "ab".repeat(16));
    assert_eq!(xmtp_proto::types::GroupId::try_from(conversation)?, raw);
    let message = MessageId::from_bytes(&[0xcd; 32])?;
    assert_eq!(MessageId::try_from(message.0.clone())?, message);
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
