use super::*;

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

/// Waits until the backend resolves `identity` to `expected`. A new client
/// looks its inbox up there, so it must see the latest association.
async fn wait_for_backend_inbox(
    identity: &PublicIdentity,
    expected: InboxId,
) -> Result<(), XmtpError> {
    let identifier = identity.to_core()?;
    let backend = options().backend.unwrap_or_default().resolve().await?;
    let api = xmtp_api::ApiClientWrapper::new(backend.api.clone(), Default::default());
    let expected = expected.into_checked()?;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let found = api
                .get_inbox_ids(vec![identifier.clone().into()])
                .await
                .map_err(XmtpError::from_api)?;
            if found.into_iter().next().flatten().as_deref() == Some(expected.as_str()) {
                return Ok::<(), XmtpError>(());
            }
            xmtp_common::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the backend did not resolve the account to the expected inbox")
}

#[xmtp_common::test(unwrap_try = true)]
async fn added_account_opens_the_existing_inbox() {
    let owner = Client::create(crate::generate_local_signer().await, options()).await?;
    let second_signer = crate::generate_local_signer().await;
    owner
        .unsafe_add_account(second_signer.clone(), false)
        .await?;
    let identity = signer::identity(second_signer.clone()).await?;
    wait_for_backend_inbox(&identity, owner.inbox_id()).await?;
    let second = Client::create(second_signer, options()).await?;
    assert_eq!(second.inbox_id(), owner.inbox_id());
    assert!(owner.inbox_state(true).await?.identities.len() >= 2);
    second.end().await?;
    owner.end().await?;
}

/// An account that belongs to another inbox moves only when the caller allows
/// reassignment. Without it, the add fails before it asks for a signature.
#[xmtp_common::test(unwrap_try = true)]
async fn account_moves_to_another_inbox_only_with_explicit_reassignment() {
    let first_owner = crate::generate_local_signer().await;
    let first = Client::create(first_owner.clone(), options()).await?;
    let next = Client::create(crate::generate_local_signer().await, options()).await?;
    let moved = signer::identity(first_owner.clone()).await?;

    let refused = next.unsafe_add_account(first_owner.clone(), false).await;
    assert!(
        matches!(&refused, Err(XmtpError::InvalidInput(details))
            if details.message == "identity belongs to another inbox"),
        "an add without reassignment must refuse, got {refused:?}"
    );
    // Only the flag stops the add: with reassignment allowed, the same
    // account gets a signature request.
    next.unsafe_add_account_signature_request(moved.clone(), true)
        .await?;

    // Free the account from the first inbox, then move it to the next inbox.
    let temporary = crate::generate_local_signer().await;
    let temporary_identity = signer::identity(temporary.clone()).await?;
    first.unsafe_add_account(temporary, true).await?;
    first
        .remove_account(first_owner.clone(), moved.clone())
        .await?;
    first
        .change_recovery_identifier(first_owner.clone(), temporary_identity.clone())
        .await?;
    let state = first.inbox_state(true).await?;
    let identifiers = state
        .identities
        .iter()
        .map(|identity| identity.identifier.as_str())
        .collect::<Vec<_>>();
    assert_eq!(identifiers, [temporary_identity.identifier.as_str()]);
    assert_eq!(
        state.recovery_identity.identifier,
        temporary_identity.identifier
    );

    next.unsafe_add_account(first_owner.clone(), true).await?;
    wait_for_backend_inbox(&moved, next.inbox_id()).await?;
    let reopened = Client::create(first_owner, options()).await?;
    assert_eq!(reopened.inbox_id(), next.inbox_id());
    reopened.end().await?;
    next.end().await?;
    first.end().await?;
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
    assert_eq!(client.libxmtp_version(), env!("CARGO_PKG_VERSION"));
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
    // The backend token is a secret; the public options omit it.
    let Some(BackendSource::Options { options: exposed }) = authenticated.options().backend else {
        panic!("test uses backend options");
    };
    assert!(
        exposed.credential.is_none(),
        "options exposed the credential"
    );
    assert!(
        exposed.credentials.is_none(),
        "options exposed the credential source"
    );
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

/// Poll `work` on a new thread with half the 512 KiB stack of a Swift
/// cooperative thread, so a call that polls the core build itself overflows.
#[cfg(not(target_arch = "wasm32"))]
async fn on_small_stack<T: Send + 'static>(
    work: impl Future<Output = T> + Send + 'static,
) -> Result<T, XmtpError> {
    let work = Box::pin(work);
    let runtime = tokio::runtime::Handle::current();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || {
            let _runtime = runtime.enter();
            let _ = sender.send(futures::executor::block_on(work));
        })
        .map_err(XmtpError::unknown)?;
    receiver.await.map_err(XmtpError::unknown)
}

// Swift polls create and build on a cooperative thread with a small stack.
#[cfg(not(target_arch = "wasm32"))]
#[xmtp_common::test(unwrap_try = true)]
async fn create_and_build_run_off_the_calling_thread() {
    let signer = crate::generate_local_signer().await;
    let identity = crate::signer::identity(signer.clone()).await?;
    let path = std::env::temp_dir().join(format!(
        "xmtp-sdk-small-stack-{}-{}.db3",
        std::process::id(),
        xmtp_common::time::now_ns()
    ));
    let mut settings = options();
    settings.storage.location = explicit_location(&path);
    let created = on_small_stack(Client::create(signer, settings.clone())).await??;
    created.end().await?;
    let built = on_small_stack(Client::build(identity, settings, None)).await??;
    assert_eq!(built.inbox_id(), created.inbox_id());
    built.end().await?;
    let _ = std::fs::remove_file(&path);
}

#[xmtp_common::test(unwrap_try = true)]
async fn passkey_signature_associates_identity_through_facade() {
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let passkey = xmtp_id::utils::passkey::PasskeyUser::new().await;
    let identity = PublicIdentity::from(passkey.get_identifier()?);
    let request = alix
        .unsafe_add_account_signature_request(identity.clone(), false)
        .await?;
    let UnverifiedSignature::Passkey(signature) = passkey.sign(&request.signature_text().await)?
    else {
        panic!("passkey fixture returned the wrong signature kind");
    };
    request
        .add_signature(Signature::Passkey {
            signature: signature.signature,
            public_key: signature.public_key,
            authenticator_data: signature.authenticator_data,
            client_data_json: signature.client_data_json,
        })
        .await?;
    alix.unsafe_apply_signature_request(request).await?;
    let state = alix.inbox_state(true).await?;
    assert!(
        state
            .identities
            .iter()
            .any(|value| value.identifier == identity.identifier
                && matches!(value.kind, PublicIdentityKind::Passkey))
    );
    alix.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn unsigned_signature_request_cannot_register() {
    let mut settings = options();
    settings.registration.auto = false;
    let client = Client::create(crate::generate_local_signer().await, settings).await?;
    let request = client
        .unsafe_create_inbox_signature_request()
        .await?
        .expect("new inbox request");
    assert!(
        client
            .unsafe_apply_signature_request(request)
            .await
            .is_err()
    );
    assert!(!client.is_registered().await?);
    client.end().await?;
}
