use super::*;

// verifies: IDENT-073
// verifies: IDENT-074
// verifies: IDENT-075
// verifies: IDENT-076
#[xmtp_common::test(unwrap_try = true)]
async fn pre_authenticate_runs_before_signing_and_propagates_failure() {
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut settings = options();
    settings.registration.auto = false;
    settings.handlers = Some(crate::ClientHandlers {
        pre_authenticate: Some(Arc::new(RecordingPreAuthenticate {
            calls: calls.clone(),
            fail: false,
        })),
    });
    let signer = Arc::new(RecordingSigner {
        key: PrivateKeySigner::random(),
        calls: calls.clone(),
    });
    let client = Client::create(signer, settings).await?;
    assert!(calls.lock().expect("calls").is_empty());
    client.register().await?;
    assert_eq!(*calls.lock().expect("calls"), ["pre-authenticate", "sign"]);
    calls.lock().expect("calls").clear();
    client.register().await?;
    assert!(calls.lock().expect("calls").is_empty());
    client.end().await?;

    calls.lock().expect("calls").clear();
    let mut settings = options();
    settings.handlers = Some(crate::ClientHandlers {
        pre_authenticate: Some(Arc::new(RecordingPreAuthenticate {
            calls: calls.clone(),
            fail: true,
        })),
    });
    let result = Client::create(
        Arc::new(RecordingSigner {
            key: PrivateKeySigner::random(),
            calls: calls.clone(),
        }),
        settings,
    )
    .await;
    assert!(matches!(result, Err(XmtpError::CallbackFailed(_))));
    assert_eq!(*calls.lock().expect("calls"), ["pre-authenticate"]);
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

#[xmtp_common::test(unwrap_try = true)]
async fn static_revoke_checks_recovery_signer_and_removes_target() {
    let signer = crate::generate_local_signer().await;
    let first = Client::create(signer.clone(), options()).await?;
    let second = Client::create(signer.clone(), options()).await?;
    let backend = options().backend.expect("backend");
    let target = second.installation_id();
    let wrong = crate::generate_local_signer().await;
    assert!(
        crate::static_helpers::revoke_installations_with_backend(
            backend.clone(),
            wrong,
            first.inbox_id(),
            vec![target.clone()]
        )
        .await
        .is_err()
    );
    assert_eq!(first.inbox_state(true).await?.installations.len(), 2);
    crate::static_helpers::revoke_installations_with_backend(
        backend,
        signer,
        first.inbox_id(),
        vec![target],
    )
    .await?;
    let state = first.inbox_state(true).await?;
    assert_eq!(state.installations.len(), 1);
    assert_eq!(state.installations[0].id, first.installation_id());
    first.end().await?;
    second.end().await?;
}
