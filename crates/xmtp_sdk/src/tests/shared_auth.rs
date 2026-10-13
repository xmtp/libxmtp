use super::*;

#[derive(Default)]
struct SwitchableCredentials {
    fail: AtomicBool,
    calls: std::sync::atomic::AtomicUsize,
}

#[xmtp_common::async_trait]
impl CredentialSource for SwitchableCredentials {
    async fn credential(&self) -> Result<Credential, CredentialError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail.load(Ordering::SeqCst) {
            return Err(CredentialError::Failed);
        }
        Ok(Credential {
            name: None,
            value: "shared-test-credential".into(),
            expires_at_seconds: xmtp_common::time::now_secs() + 300,
        })
    }
}

// verifies: AUTH-035, EVENT-055
#[xmtp_common::test(unwrap_try = true)]
async fn built_clients_share_lockout_transitions_once_and_reset_together() {
    let credentials = Arc::new(SwitchableCredentials::default());
    let backend = Arc::new(
        crate::Backend::connect(BackendOptions {
            url: xmtp_configuration::backend_test_url(),
            credentials: Some(credentials.clone()),
            ..Default::default()
        })
        .await?,
    );
    let mut settings = options();
    settings.registration.auto = false;
    settings.backend = Some(BackendSource::Connected {
        backend: backend.clone(),
    });
    let first = Client::create(crate::generate_local_signer().await, settings.clone()).await?;
    let second = Client::create(crate::generate_local_signer().await, settings).await?;
    let readers = [
        first
            .events(event_filter(vec![EventKind::ClientLockoutChanged]))
            .await?,
        second
            .events(event_filter(vec![EventKind::ClientLockoutChanged]))
            .await?,
    ];
    credentials.fail.store(true, Ordering::SeqCst);
    backend
        .auth_handle
        .as_ref()?
        .set(
            Credential {
                name: None,
                value: "expired".into(),
                expires_at_seconds: 0,
            }
            .to_backend()?,
        )
        .await;
    // One outer operation can retry. Alternate real client calls until the shared handle locks.
    let mut locked = false;
    for attempt in 0..xmtp_configuration::MAX_CONSECUTIVE_AUTH_FAILURES {
        let client = if attempt % 2 == 0 { &first } else { &second };
        let result = client.can_message(vec![client.identity()]).await;
        if matches!(result, Err(XmtpError::CredentialExhausted(_))) {
            locked = true;
            break;
        }
        assert!(
            matches!(result, Err(XmtpError::CredentialCallbackFailed(_))),
            "{result:?}"
        );
    }
    assert!(locked);
    let calls = credentials.calls.load(Ordering::SeqCst);
    for client in [&first, &second] {
        assert!(matches!(
            client.can_message(vec![client.identity()]).await,
            Err(XmtpError::CredentialExhausted(_))
        ));
    }
    assert_eq!(credentials.calls.load(Ordering::SeqCst), calls);
    for reader in &readers {
        let event = xmtp_common::time::timeout(Duration::from_secs(2), reader.next()).await??;
        assert!(
            matches!(event, Some(ClientEvent::ClientLockoutChanged { lockout_changed }) if lockout_changed.change == crate::LockoutChange::Entered)
        );
    }
    credentials.fail.store(false, Ordering::SeqCst);
    first
        .set_credential(Credential {
            name: None,
            value: "replacement".into(),
            expires_at_seconds: xmtp_common::time::now_secs() + 300,
        })
        .await?;
    for client in [&first, &second] {
        client.can_message(vec![client.identity()]).await?;
    }
    for reader in &readers {
        let event = xmtp_common::time::timeout(Duration::from_secs(2), reader.next()).await??;
        assert!(
            matches!(event, Some(ClientEvent::ClientLockoutChanged { lockout_changed }) if lockout_changed.change == crate::LockoutChange::Left)
        );
        assert!(
            xmtp_common::time::timeout(Duration::from_millis(50), reader.next())
                .await
                .is_err()
        );
        reader.end().await?;
    }
    first.end().await?;
    second.end().await?;
}
