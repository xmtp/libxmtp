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
