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
    BackendOptions, BackendSource, Client, ClientOptions, ConversationID, Credential,
    CredentialError, CredentialSource, InboxID, MessageContent, MessageID, PublicIdentity,
    PublicIdentityKind, Signature, Signer, SignerError, SignerKind, SigningRequest,
    StorageLocation, StorageOptions, XmtpError, client::native_storage_path,
    credentials::AuthBridge, reader, signer,
};

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

#[xmtp_common::test(unwrap_try = true)]
async fn added_account_opens_the_existing_inbox() {
    let owner = Client::create(crate::generate_local_signer().await, options()).await?;
    let second_signer = crate::generate_local_signer().await;
    owner
        .unsafe_add_account(second_signer.clone(), false)
        .await?;
    let identifier = signer::identity(second_signer.clone()).await?.to_core()?;
    let backend = options().backend.unwrap_or_default().resolve().await?;
    let api = xmtp_api::ApiClientWrapper::new(backend.api.clone(), Default::default());
    let expected = owner.inbox_id().0;
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
    .expect("added account did not become visible to the backend")?;
    let second = Client::create(second_signer, options()).await?;
    assert_eq!(second.inbox_id(), owner.inbox_id());
    assert!(owner.inbox_state(true).await?.identities.len() >= 2);
    second.end().await?;
    owner.end().await?;
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

#[xmtp_common::test]
fn notification_and_auth_errors_keep_their_kinds() {
    use crate::ErrorCategory;
    use xmtp_mls::client::notifications::NotificationError as N;
    use xmtp_proto::api::AuthError as A;

    macro_rules! notification {
        ($source:expr, $variant:ident, $category:pat, $retryable:expr) => {{
            let mapped = XmtpError::from_notification($source);
            let XmtpError::$variant(details) = mapped else {
                panic!("notification error became {mapped:?}");
            };
            assert_eq!(details.code, stringify!($variant));
            assert!(matches!(details.category, $category));
            assert_eq!(details.retryable, $retryable);
        }};
    }
    notification!(
        N::TaskRunnerDisabled,
        TaskRunnerDisabled,
        ErrorCategory::Notification,
        false
    );
    notification!(
        N::PermissionDenied,
        PermissionDenied,
        ErrorCategory::Notification,
        false
    );
    notification!(
        N::InvalidArgument,
        InvalidArgument,
        ErrorCategory::Notification,
        false
    );
    notification!(
        N::OutOfRange,
        OutOfRange,
        ErrorCategory::Notification,
        false
    );
    notification!(
        N::Unimplemented,
        Unimplemented,
        ErrorCategory::Notification,
        false
    );
    notification!(
        N::ChannelNotConfigured,
        ChannelNotConfigured,
        ErrorCategory::Notification,
        false
    );
    notification!(
        N::ResourceExhausted,
        ResourceExhausted,
        ErrorCategory::Notification,
        false
    );
    notification!(
        N::RequestTimeout,
        RequestTimeout,
        ErrorCategory::Notification,
        true
    );
    notification!(
        N::NotFound,
        NotificationNotFound,
        ErrorCategory::Notification,
        true
    );
    notification!(
        N::Api(xmtp_api::ApiError::InvalidRequest("test")),
        NotificationApi,
        ErrorCategory::Notification,
        false
    );
    notification!(
        N::Storage(xmtp_db::StorageError::DbDeserialize),
        NotificationStorage,
        ErrorCategory::Storage,
        false
    );
    notification!(
        N::Group(xmtp_mls::groups::GroupError::UserLimitExceeded),
        NotificationGroup,
        ErrorCategory::Conversation,
        false
    );

    let auth_cases = [
        (
            A::CredentialRejected { retryable: true },
            "CredentialRejected",
            true,
        ),
        (
            A::CallbackFailed { retryable: true },
            "CredentialCallbackFailed",
            true,
        ),
        (A::Exhausted, "CredentialExhausted", false),
        (A::ExhaustedAfterAttempt, "CredentialExhausted", false),
        (A::MissingCredential, "CredentialMissing", false),
    ];
    for (source, code, retryable) in auth_cases {
        let mapped = XmtpError::from_api(xmtp_api::ApiError::Auth(source));
        let details = match mapped {
            XmtpError::CredentialRejected(details)
            | XmtpError::CredentialCallbackFailed(details)
            | XmtpError::CredentialExhausted(details)
            | XmtpError::CredentialMissing(details) => details,
            other => panic!("auth error became {other:?}"),
        };
        assert_eq!(details.code, code);
        assert!(matches!(details.category, ErrorCategory::Callback));
        assert_eq!(details.retryable, retryable);
    }
    let nested = xmtp_mls::builder::ClientBuilderError::Identity(
        xmtp_mls::identity::IdentityError::ApiClient(xmtp_api::ApiError::Auth(A::CallbackFailed {
            retryable: true,
        })),
    );
    assert!(matches!(
        XmtpError::from_builder(nested),
        XmtpError::CredentialCallbackFailed(details) if details.retryable
    ));
}

#[xmtp_common::test(unwrap_try = true)]
fn out_of_range_installation_time_does_not_fail_inbox_state() {
    use xmtp_id::associations::{AssociationState, Identifier, Member, MemberIdentifier};
    let owner = Identifier::eth("0x1111111111111111111111111111111111111111")?;
    let installation = MemberIdentifier::installation(vec![1; 32]);
    let state = AssociationState::new(owner, 0, None)?.add(Member::new(
        installation,
        None,
        Some(u64::MAX),
        None,
    ));
    let state = crate::InboxState::from_core(state, None)?;
    assert_eq!(state.installations.len(), 1);
    assert_eq!(
        state.installations[0].created_at_ns,
        Some(crate::Timestamp(i64::MAX))
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn backend_only_identity_and_message_queries() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let Some(BackendSource::Options {
        options: backend_options,
    }) = options().backend
    else {
        panic!("test uses backend options");
    };
    let backend = Arc::new(crate::Backend::connect(backend_options).await?);
    let source = BackendSource::Connected { backend };
    let identity = client.identity();
    let inbox =
        crate::static_helpers::inbox_id_for_with_backend(source.clone(), identity.clone()).await?;
    assert_eq!(inbox, client.inbox_id());
    let availability =
        crate::static_helpers::can_message_with_backend(source.clone(), vec![identity.clone()])
            .await?;
    assert!(availability[0].can_message);
    let states =
        crate::static_helpers::inbox_states_with_backend(source.clone(), vec![inbox.clone()])
            .await?;
    assert_eq!(states[0].inbox_id, inbox);
    assert!(
        crate::static_helpers::is_address_authorized_with_backend(
            source.clone(),
            inbox.clone(),
            identity.identifier,
        )
        .await?
    );
    assert!(
        crate::static_helpers::is_installation_authorized_with_backend(
            source.clone(),
            inbox,
            client.installation_id(),
        )
        .await?
    );
    let group = client.conversations().create_group(vec![], None).await?;
    group.send_text("metadata".into()).await?;
    let metadata = crate::static_helpers::newest_message_metadata_with_backend(
        source.clone(),
        vec![group.id()],
    )
    .await?;
    assert_eq!(metadata.len(), 1);
    let connected_client = Client::build(
        client.identity(),
        ClientOptions {
            backend: Some(source),
            ..options()
        },
        Some(client.inbox_id()),
    )
    .await?;
    assert_eq!(connected_client.inbox_id(), client.inbox_id());
    connected_client.end().await?;
    client.end().await?;
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
        .create_group(vec![bo.inbox_id()], None)
        .await?;
    bo.inner.sync_welcomes().await?;
    let bo_group = crate::Group::from_core(bo.inner.group(&group.inner.group_id)?, bo.key).await?;
    let id = group.send_text("hello from the slice".into()).await?;
    let history = group.messages(None).await?;
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
    let group = client.conversations().create_group(vec![], None).await?;
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
        pool: None,
        single_connection: false,
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
fn wasm_directory_reports_the_store_path() {
    let options = StorageOptions {
        location: StorageLocation::Directory("sdk-files".into()),
        label: Some("phone".into()),
        ..Default::default()
    };
    let reported = crate::client::wasm_storage_path(&options, "inbox-a")?.expect("file path");
    let location = crate::client::wasm_store_location(&options, "inbox-a")?;
    let xmtp_db::StorageOption::Persistent(opened) = &location else {
        panic!("Directory storage must be persistent");
    };
    assert_eq!(reported, opened.as_str());
    assert_eq!(reported, "sdk-files/xmtp-phone-inbox-a.db3");
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
    let group = client.conversations().create_group(vec![], None).await?;
    client.end().await?;
    assert!(matches!(
        group.send_text("after end".into()).await,
        Err(XmtpError::ClientClosed(_))
    ));
    assert!(matches!(
        group.messages(None).await,
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
        client.conversations().create_group(vec![], None).await,
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
    let group = client.conversations().create_group(vec![], None).await?;
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
    let group = client.conversations().create_group(vec![], None).await?;
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
    let group = client.conversations().create_group(vec![], None).await?;
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
    let stale_group = client.conversations().create_group(vec![], None).await?;
    let live_group = client.conversations().create_group(vec![], None).await?;
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

#[xmtp_common::test(unwrap_try = true)]
async fn conversation_list_state_and_last_activity() {
    use crate::{Conversation, ConversationOrder, ListConversationsOptions};
    use xmtp_db::{count_sql_queries, sql_key_store::count_kv_reads};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let older = client.conversations().create_group(vec![], None).await?;
    let newer = client.conversations().create_group(vec![], None).await?;
    let sent = older.send_text("most recent".into()).await?;
    let stored_sent_at_ns = client.inner.message(hex::decode(&sent.0)?)?.sent_at_ns;
    let ordered = client
        .conversations()
        .list(Some(ListConversationsOptions {
            order_by: Some(ConversationOrder::LastActivity),
            ..Default::default()
        }))
        .await?;
    let ids = ordered
        .into_iter()
        .map(|conversation| match conversation {
            Conversation::Group { group } => group.id(),
            Conversation::Dm { dm } => dm.id(),
        })
        .collect::<Vec<_>>();
    let core_ids = client
        .inner
        .list_conversations(xmtp_db::group::GroupQueryArgs {
            order_by: Some(xmtp_db::group::GroupQueryOrderBy::LastActivity),
            ..Default::default()
        })?
        .into_iter()
        .map(|item| ConversationID::from(item.group.group_id))
        .collect::<Vec<_>>();
    assert_eq!(ids, core_ids);
    let older_activity = older.last_activity_at_ns(None).await?.0;
    assert_eq!(older_activity, stored_sent_at_ns);
    let newer_activity = newer.last_activity_at_ns(None).await?.0;
    let first_activity = if ids.first() == Some(&older.id()) {
        older_activity
    } else {
        assert_eq!(ids.first(), Some(&newer.id()));
        newer_activity
    };
    let second_activity = if ids.get(1) == Some(&older.id()) {
        older_activity
    } else {
        assert_eq!(ids.get(1), Some(&newer.id()));
        newer_activity
    };
    assert!(first_activity >= second_activity);
    assert_eq!(
        older.last_activity_at_ns(Some(vec![])).await?,
        older.created_at()
    );
    let text_type = crate::encode_text("filter".into())?.r#type;
    assert_eq!(
        older.last_activity_at_ns(Some(vec![text_type])).await?,
        older.last_activity_at_ns(None).await?
    );

    let ((snapshot, core_kv_reads), core_queries, core_writes) =
        count_sql_queries(|| count_kv_reads(|| older.inner.state_snapshot()));
    let snapshot = snapshot?;
    let facade_state = older.state().await?;
    assert_eq!(
        facade_state.name,
        snapshot.group.expect("group metadata").name
    );
    let (queries, kv_reads, writes) = *older.state_counts.lock();
    assert!(
        queries <= 3,
        "facade state read used {queries} SQL queries and {kv_reads} key reads"
    );
    assert!(kv_reads <= 2, "state read used {kv_reads} key-value reads");
    assert_eq!(writes, 0, "state read began a write transaction");
    assert!(core_queries.saturating_sub(core_kv_reads) <= 3);
    assert!(core_kv_reads <= 2);
    assert_eq!(core_writes, 0);
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn conversation_list_lift_uses_bounded_queries() {
    use xmtp_db::{count_sql_queries, sql_key_store::count_kv_reads};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    for _ in 0..3 {
        client.conversations().create_group(vec![], None).await?;
    }
    let ((result, kv_reads), queries, writes) = count_sql_queries(|| {
        count_kv_reads(|| {
            futures::executor::block_on(crate::conversation::list_local(
                client.inner.clone(),
                client.key,
                Default::default(),
            ))
        })
    });
    assert!(result?.len() >= 3);
    assert!(queries <= 3, "list used {queries} SQL queries");
    assert!(kv_reads <= 1, "list used {kv_reads} key-value reads");
    assert_eq!(writes, 0);
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn message_history_queries_do_not_grow_per_row() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let first = group.send_text("first".into()).await?;
    assert_eq!(group.messages(None).await?.len(), 1);
    let one_query_count = *group.history_query_count.lock();

    for number in 0..3 {
        group.send_text(format!("more {number}")).await?;
    }
    client
        .conversations()
        .reply_to_message(first, crate::encode_text("reply".into())?, None)
        .await?;
    assert_eq!(group.messages(None).await?.len(), 5);
    let many_query_count = *group.history_query_count.lock();
    assert!(
        many_query_count <= one_query_count + 2,
        "history used {one_query_count} queries for one row and {many_query_count} for five rows"
    );
    client.end().await?;
}

// verifies: CTYPE-010, SEND-021
#[xmtp_common::test(unwrap_try = true)]
async fn encoded_sends_use_catalogue_push_defaults_and_explicit_override() {
    use crate::{Reaction, ReactionAction, ReactionSchema, SendOptions};
    use xmtp_content_types::{ContentCodec, reaction::ReactionCodec};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let parent = group.send_text("reference".into()).await?;
    let reaction = || Reaction {
        content: "👍".into(),
        action: ReactionAction::Added,
        schema: ReactionSchema::Unicode,
    };
    let encoded_reaction = || {
        ReactionCodec::encode(reaction().into_proto(parent.clone(), client.inbox_id()))
            .map(Into::into)
    };
    let stored_push = |id: &MessageID| {
        client
            .inner
            .message(hex::decode(&id.0).expect("message ID"))
            .expect("stored message")
            .should_push
    };

    let raw = group.send(encoded_reaction()?, None).await?;
    let raw_push = stored_push(&raw);
    let action = client
        .conversations()
        .react_to_message(parent.clone(), reaction(), None)
        .await?;
    let action_push = stored_push(&action);
    let prepared = group.prepare_message(encoded_reaction()?, None).await?;
    let prepared_push = stored_push(&prepared);
    let text = group.send(crate::encode_text("text".into())?, None).await?;
    let text_push = stored_push(&text);
    let override_id = group
        .send(
            encoded_reaction()?,
            Some(SendOptions {
                should_push: Some(true),
                ..Default::default()
            }),
        )
        .await?;
    let override_push = stored_push(&override_id);
    let reply = client
        .conversations()
        .reply_to_message(parent, crate::encode_text("reply".into())?, None)
        .await?;
    let reply_push = stored_push(&reply);
    let observed = [
        raw_push,
        action_push,
        prepared_push,
        text_push,
        override_push,
        reply_push,
    ];
    assert_eq!(
        observed,
        [false, false, false, true, true, true],
        "raw, action, and prepared reactions must use the catalogue push default"
    );
    client.end().await?;
}

async fn assert_undecodable_standard_read_paths(
    client: &Client,
    group: &Arc<crate::Group>,
    id: MessageID,
    expected_raw: &[u8],
) -> Result<(), XmtpError> {
    let stored = client
        .inner
        .message(hex::decode(&id.0).map_err(XmtpError::unknown)?)
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

// verifies: CTYPE-008, CTYPE-009
#[xmtp_common::test(unwrap_try = true)]
async fn invalid_text_bytes_stay_unknown_on_all_read_paths() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let mut encoded = crate::encode_text("valid".into())?;
    encoded.content = vec![0xff, 0xfe];
    let id = group.send(encoded, None).await?;
    let raw = client
        .inner
        .message(hex::decode(&id.0)?)?
        .decrypted_message_bytes;
    assert_undecodable_standard_read_paths(&client, &group, id, &raw).await?;
    client.end().await?;
}

// verifies: CTYPE-007, CTYPE-008
#[xmtp_common::test(unwrap_try = true)]
async fn actions_with_out_of_range_expiry_stay_unknown_on_all_read_paths() {
    use xmtp_content_types::{
        ContentCodec,
        actions::{Actions, ActionsCodec},
    };

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let actions: Actions = serde_json::from_str(
        r#"{"id":"far-future","description":"Choose","expiresAt":"9999-12-31T23:59:59.999Z","actions":[{"id":"one","label":"One","expiresAt":"9999-12-31T23:59:59.999Z"}]}"#,
    )?;
    let mut action_only = actions.clone();
    action_only.expires_at = None;
    for actions in [actions, action_only] {
        let encoded = ActionsCodec::encode(actions)?;
        let id = group.send(encoded.into(), None).await?;
        let stored = client.inner.message(hex::decode(&id.0)?)?;
        let raw = stored.decrypted_message_bytes.clone();
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
        for (path, message) in [("stored", direct), ("by ID", by_id), ("history", history)] {
            assert!(
                matches!(message.0.content, MessageContent::Unknown { encoded, raw_bytes }
                    if encoded.r#type.type_id == "actions" && raw_bytes == raw),
                "{path} changed an out-of-range Actions expiry"
            );
        }
    }
    client.end().await?;
}

// verifies: CTYPE-008, CTYPE-024
#[xmtp_common::test(unwrap_try = true)]
async fn unknown_compression_stays_unknown_on_all_read_paths() {
    use prost::Message as _;
    use xmtp_db::{ConnectionExt, diesel::prelude::*, schema::group_messages::dsl};
    use xmtp_proto::xmtp::mls::message_contents::EncodedContent as ProtoEncodedContent;

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let id = group.send_text("valid".into()).await?;
    let id_bytes = hex::decode(&id.0)?;
    let stored = client.inner.message(id_bytes.clone())?;
    let mut encoded = ProtoEncodedContent::decode(stored.decrypted_message_bytes.as_slice())?;
    encoded.compression = Some(99);
    let raw = encoded.encode_to_vec();
    client.inner.context.db().raw_query(|conn| {
        xmtp_db::diesel::update(dsl::group_messages.filter(dsl::id.eq(id_bytes)))
            .set(dsl::decrypted_message_bytes.eq(&raw))
            .execute(conn)
    })?;
    assert_undecodable_standard_read_paths(&client, &group, id, &raw).await?;
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn nested_reaction_reply_body_keeps_nested_envelope() {
    use crate::{EncodedContent, MessageBody, Reaction, ReactionAction, ReactionSchema};
    use prost::Message as _;
    use xmtp_content_types::{ContentCodec, reaction::ReactionCodec};
    use xmtp_proto::xmtp::mls::message_contents::EncodedContent as ProtoEncodedContent;

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let reference = group.send_text("reference".into()).await?;
    let nested: EncodedContent = ReactionCodec::encode(
        Reaction {
            content: "👍".into(),
            action: ReactionAction::Added,
            schema: ReactionSchema::Unicode,
        }
        .into_proto(reference.clone(), client.inbox_id()),
    )?
    .into();
    let reply_id = client
        .conversations()
        .reply_to_message(reference, nested, None)
        .await?;
    let stored = client.inner.message(hex::decode(&reply_id.0)?)?;
    let decoded = client
        .decode_content(
            ProtoEncodedContent::decode(stored.decrypted_message_bytes.as_slice())?.into(),
        )
        .await?;
    let MessageContent::Reply {
        body: MessageBody::Unknown { encoded },
        ..
    } = decoded
    else {
        panic!("expected an unknown nested reaction body");
    };
    assert_eq!(encoded.r#type.type_id, "reaction");

    let reply = group
        .messages(None)
        .await?
        .into_iter()
        .find(|message| message.0.id == reply_id)
        .expect("reply in history");
    let MessageContent::Reply {
        body: MessageBody::Unknown { encoded },
        ..
    } = reply.0.content
    else {
        panic!("expected an unknown nested reaction body in history");
    };
    assert_eq!(encoded.r#type.type_id, "reaction");
    client.end().await?;
}

// verifies: CTYPE-023
#[xmtp_common::test(unwrap_try = true)]
async fn message_actions_use_ids_and_compression_is_opt_in() {
    use crate::{
        Compression, EncodedContent, Reaction, ReactionAction, ReactionSchema, SendOptions,
    };
    use prost::Message as _;
    use xmtp_content_types::{ContentCodec, text::TextCodec};
    use xmtp_proto::xmtp::mls::message_contents::EncodedContent as ProtoEncodedContent;

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let text: EncodedContent = TextCodec::encode("plain".into())?.into();
    let plain = group.send(text, None).await?;
    let stored = client.inner.message(hex::decode(&plain.0)?)?;
    assert_eq!(
        ProtoEncodedContent::decode(stored.decrypted_message_bytes.as_slice())?.compression,
        None
    );

    let compressed: EncodedContent = TextCodec::encode("compressed".into())?.into();
    let compressed_id = group
        .send(
            compressed,
            Some(SendOptions {
                compression: Some(Compression::Gzip),
                ..Default::default()
            }),
        )
        .await?;
    let stored = client.inner.message(hex::decode(&compressed_id.0)?)?;
    assert!(matches!(
        MessageContent::decode(stored.decrypted_message_bytes.clone())?,
        MessageContent::Text(value) if value == "compressed"
    ));
    assert!(
        ProtoEncodedContent::decode(stored.decrypted_message_bytes.as_slice())?
            .compression
            .is_some()
    );
    let read_back = client
        .conversations()
        .get_message_by_id(compressed_id)
        .await?
        .expect("compressed message");
    assert!(matches!(read_back.0.content, MessageContent::Text(value) if value == "compressed"));
    assert!(
        client
            .conversations()
            .get_message_by_id(plain.clone())
            .await?
            .is_some()
    );

    let reaction_id = client
        .conversations()
        .react_to_message(
            plain.clone(),
            Reaction {
                content: "👍".into(),
                action: ReactionAction::Added,
                schema: ReactionSchema::Unicode,
            },
            None,
        )
        .await?;
    let reply: EncodedContent = TextCodec::encode("answer".into())?.into();
    let reply_id = client
        .conversations()
        .reply_to_message(plain.clone(), reply, None)
        .await?;
    assert_ne!(reaction_id, reply_id);
    assert_ne!(plain, reply_id);
    let enriched = group.messages(None).await?;
    let original = enriched
        .iter()
        .find(|value| value.0.id == plain)
        .expect("original message");
    assert_eq!(original.0.reply_count, 1);
    assert_eq!(original.0.reactions.len(), 1);
    assert_eq!(original.0.reactions[0].id, reaction_id);
    let answer = enriched
        .iter()
        .find(|value| value.0.id == reply_id)
        .expect("reply message");
    assert_eq!(
        answer.0.in_reply_to.as_ref().map(|parent| &parent.id),
        Some(&plain)
    );
    assert!(
        !answer
            .0
            .in_reply_to
            .as_ref()
            .expect("parent")
            .encoded
            .content
            .is_empty()
    );
    let local_message = group.send_text("delete through group".into()).await?;
    assert_ne!(
        group.delete_message(local_message.clone()).await?,
        local_message
    );
    let deleted = client.conversations().delete_message(plain.clone()).await?;
    assert_ne!(deleted, plain);
    client.end().await?;
}

// verifies: CTYPE-008
#[xmtp_common::test(unwrap_try = true)]
async fn unknown_message_bytes_remain_available_to_the_host() {
    use prost::Message as _;
    use xmtp_proto::xmtp::mls::message_contents::EncodedContent as ProtoEncodedContent;

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let id = group.send_text("fallback".into()).await?;
    let mut stored = client.inner.message(hex::decode(&id.0)?)?;
    let original = vec![0xff, 0x00, 0x80];
    stored.decrypted_message_bytes = original.clone();
    let message = crate::Message::from_stored(stored, client.client_key())?;
    let MessageContent::Unknown { raw_bytes, .. } = &message.0.content else {
        panic!("untyped bytes must remain unknown");
    };
    assert_eq!(raw_bytes, &original);
    assert_eq!(message.0.encoded.content, original);

    let mut stored = client.inner.message(hex::decode(&id.0)?)?;
    let mut proto = ProtoEncodedContent::decode(stored.decrypted_message_bytes.as_slice())?;
    proto.compression = Some(12_345);
    let original_content = proto.content.clone();
    let original_bytes = proto.encode_to_vec();
    stored.decrypted_message_bytes = original_bytes.clone();
    let message = crate::Message::from_stored(stored, client.client_key())?;
    let MessageContent::Unknown { encoded, raw_bytes } = &message.0.content else {
        panic!("unknown compression must remain unknown");
    };
    assert_eq!(raw_bytes, &original_bytes);
    assert!(encoded.content.is_empty());
    assert!(message.0.encoded.content.is_empty());
    assert!(!original_content.is_empty());
    client.end().await?;
}

// verifies: CTYPE-024, CTYPE-025
#[xmtp_common::test(unwrap_try = true)]
fn decode_rejects_compression_bomb_with_bounded_output() {
    use flate2::{Compression as FlateCompression, write::ZlibEncoder};
    use prost::Message as _;
    use std::io::Write;
    use xmtp_content_types::{
        ContentCodec,
        compression::{COMPRESSION_CHUNK_BYTES, DecompressionBudget, MAX_DECOMPRESSED_BYTES},
        text::TextCodec,
    };
    use xmtp_proto::xmtp::mls::message_contents::Compression as WireCompression;

    let mut encoder = ZlibEncoder::new(Vec::new(), FlateCompression::default());
    encoder.write_all(&vec![b'x'; MAX_DECOMPRESSED_BYTES + 1])?;
    let mut content = TextCodec::encode("placeholder".into())?;
    content.content = encoder.finish()?;
    content.compression = Some(WireCompression::Deflate as i32);
    let error = MessageContent::decode(content.clone().encode_to_vec()).unwrap_err();
    assert!(
        error.to_string().contains("decompressed content exceeds"),
        "unexpected decode error: {error}"
    );
    let mut budget = DecompressionBudget::new();
    assert!(xmtp_content_types::compression::decompress_with_budget(content, &mut budget).is_err());
    assert!(budget.peak_capacity() <= MAX_DECOMPRESSED_BYTES + COMPRESSION_CHUNK_BYTES);
}

#[xmtp_common::test(unwrap_try = true)]
async fn consent_archive_storage_and_diagnostics() {
    use crate::{ConsentEntity, ConsentRecord, ConsentState};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let preferences = client.preferences();
    let entity = ConsentEntity::Inbox {
        inbox_id: client.inbox_id(),
    };
    preferences
        .set_consent_states(vec![ConsentRecord {
            entity: entity.clone(),
            state: ConsentState::Allowed,
        }])
        .await?;
    assert!(matches!(
        preferences.consent_state(entity).await?,
        ConsentState::Allowed
    ));
    assert!(client.storage().path().await?.is_none());
    client.diagnostics().clear_statistics().await?;
    let stats = client.diagnostics().api_statistics().await?;
    assert_eq!(stats.query, 0);
    let archive = client.archives().export_to_bytes(vec![7; 32], None).await?;
    assert!(!archive.is_empty());
    let metadata = client
        .archives()
        .metadata_from_bytes(archive, vec![7; 32])
        .await?;
    assert_eq!(metadata.backup_version, 0);
    client.end().await?;
}

// verifies: ARCH-017
#[xmtp_common::test(unwrap_try = true)]
async fn explicit_empty_archive_elements_export_nothing() {
    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let bytes = client
        .archives()
        .export_to_bytes(
            vec![7; 32],
            Some(crate::ArchiveOptions {
                start: None,
                end: None,
                elements: Some(vec![]),
                exclude_disappearing_messages: false,
            }),
        )
        .await?;
    let metadata = client
        .archives()
        .metadata_from_bytes(bytes, vec![7; 32])
        .await?;
    assert!(metadata.elements.is_empty());
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn group_options_metadata_members_and_message_filters() {
    use crate::{CreateGroupOptions, GroupPermissionMode, ListMessagesOptions, MessageOrder};

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client
        .conversations()
        .create_group(
            vec![],
            Some(CreateGroupOptions {
                permissions: Some(GroupPermissionMode::AdminOnly),
                name: Some("first name".into()),
                ..Default::default()
            }),
        )
        .await?;
    let state = group.state().await?;
    assert_eq!(state.name, "first name");
    let (_, immutable_reads, _) = xmtp_db::count_sql_queries(|| {
        assert_eq!(group.creator_inbox_id(), client.inbox_id());
        assert_eq!(group.added_by_inbox_id(), client.inbox_id());
        assert!(group.is_creator());
        assert!(!group.topic().is_empty());
    });
    assert_eq!(immutable_reads, 0, "immutable fields read the database");
    assert!(matches!(
        state.permissions.policy_type,
        crate::GroupPolicyType::AdminOnly
    ));
    assert!(
        group
            .members()
            .await?
            .iter()
            .any(|member| member.inbox_id == client.inbox_id())
    );
    let debug = group.debug_info().await?;
    assert!(!debug.cursor.is_empty());
    let capabilities = group.membership_capabilities().await?;
    assert!(capabilities.members.iter().any(|member| {
        member.inbox_id == client.inbox_id()
            && member
                .installations
                .iter()
                .any(|installation| installation.is_own)
    }));
    group.update_name("second name".into()).await?;
    assert_eq!(group.state().await?.name, "second name");
    let first = group.send_text("first".into()).await?;
    let second = group.send_text("second".into()).await?;
    let messages = group
        .messages(Some(ListMessagesOptions {
            limit: Some(1),
            direction: Some(MessageOrder::Descending),
            ..Default::default()
        }))
        .await?;
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].0.id, second);
    assert_ne!(first, second);
    client.end().await?;
}

// verifies: DMS-007
#[xmtp_common::test(unwrap_try = true)]
async fn duplicate_dm_message_actions_keep_typed_results() {
    use crate::{Reaction, ReactionAction, ReactionSchema};

    let a = Client::create(crate::generate_local_signer().await, options()).await?;
    let b = Client::create(crate::generate_local_signer().await, options()).await?;
    let first_dm = a.conversations().create_dm(b.inbox_id(), None).await?;
    let first = first_dm.send_text("first duplicate".into()).await?;
    let second_dm = b.conversations().create_dm(a.inbox_id(), None).await?;
    let second = second_dm.send_text("second duplicate".into()).await?;
    b.conversations().sync_all(None).await?;
    a.conversations().sync_all(None).await?;

    let mut inactive = None;
    for id in [first.clone(), second.clone()] {
        let bytes = hex::decode(&id.0)?;
        if let Some((stored, stitched)) = a.inner.message_with_group(&bytes).await?
            && stored.group_id != stitched.group_id
        {
            inactive = Some(id);
            break;
        }
    }
    let id = inactive.expect("one duplicate DM must be inactive");
    let owner = if id == first { &a } else { &b };
    let stored = owner.inner.message(hex::decode(&id.0)?)?;
    assert_eq!(stored.sender_inbox_id, owner.inbox_id().0);
    let active_id = if id == first { &second } else { &first };
    let active_group_id = owner.inner.message(hex::decode(&active_id.0)?)?.group_id;
    let crate::Conversation::Dm { dm: resolved_dm } = owner
        .conversations()
        .get_by_id(stored.group_id.into())
        .await?
        .expect("inactive DM resolves to the active DM")
    else {
        panic!("expected a DM");
    };
    assert_eq!(resolved_dm.id(), active_group_id.into());
    let peer_message = active_id.clone();
    owner.conversations().sync_all(None).await?;
    let crate::Conversation::Dm { dm: active_dm } = owner
        .conversations()
        .get_by_id(active_group_id.into())
        .await?
        .expect("active duplicate DM")
    else {
        panic!("expected a DM");
    };
    active_dm
        .send_text("keep other duplicate active".into())
        .await?;
    for message_id in [&id] {
        let bytes = hex::decode(&message_id.0)?;
        let (stored, winner) = owner
            .inner
            .message_with_group(&bytes)
            .await?
            .expect("message");
        assert_ne!(stored.group_id, winner.group_id);
    }
    let reaction = owner
        .conversations()
        .react_to_message(
            id.clone(),
            Reaction {
                content: "👍".into(),
                action: ReactionAction::Added,
                schema: ReactionSchema::Unicode,
            },
            None,
        )
        .await?;
    let reply = owner
        .conversations()
        .reply_to_message(id.clone(), crate::encode_text("reply".into())?, None)
        .await?;
    assert_ne!(reaction, reply);
    let peer_error = owner
        .conversations()
        .delete_message(peer_message)
        .await
        .unwrap_err();
    assert!(
        matches!(&peer_error, crate::XmtpError::PermissionDenied(details) if details.message.contains("not your message")),
        "{peer_error:?}"
    );
    assert_ne!(owner.conversations().delete_message(id.clone()).await?, id);
    a.end().await?;
    b.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
fn standard_content_types_decode_to_records() {
    use prost::Message as _;
    use xmtp_content_types::{
        ContentCodec,
        actions::{Action, Actions, ActionsCodec},
        attachment::{Attachment, AttachmentCodec},
        group_updated::GroupUpdatedCodec,
        intent::{Intent, IntentCodec},
        leave_request::LeaveRequestCodec,
        remote_attachment::{RemoteAttachment, RemoteAttachmentCodec},
        transaction_reference::{TransactionReference, TransactionReferenceCodec},
        wallet_send_calls::{WalletSendCalls, WalletSendCallsCodec},
    };
    use xmtp_proto::xmtp::mls::message_contents::{GroupUpdated, content_types::LeaveRequest};

    let attachment = Attachment {
        filename: Some("file.txt".into()),
        mime_type: "text/plain".into(),
        content: b"data".to_vec(),
    };
    assert!(
        matches!(MessageContent::decode(AttachmentCodec::encode(attachment)?.encode_to_vec())?, MessageContent::Attachment(value) if value.content == b"data")
    );
    let remote = RemoteAttachment {
        url: "https://example.org/file".into(),
        content_digest: "abc".into(),
        secret: vec![1],
        salt: vec![2],
        nonce: vec![3],
        scheme: "https".into(),
        content_length: Some(1),
        filename: None,
    };
    assert!(
        matches!(MessageContent::decode(RemoteAttachmentCodec::encode(remote)?.encode_to_vec())?, MessageContent::RemoteAttachment(value) if value.url == "https://example.org/file")
    );
    let transaction = TransactionReference {
        namespace: None,
        network_id: "1".into(),
        reference: "0xabc".into(),
        metadata: None,
    };
    assert!(
        matches!(MessageContent::decode(TransactionReferenceCodec::encode(transaction)?.encode_to_vec())?, MessageContent::TransactionReference(value) if value.network_id == "1")
    );
    let calls = WalletSendCalls {
        version: "1".into(),
        chain_id: "1".into(),
        from: "0x1".into(),
        calls: vec![],
        capabilities: None,
    };
    assert!(
        matches!(MessageContent::decode(WalletSendCallsCodec::encode(calls)?.encode_to_vec())?, MessageContent::WalletSendCalls(value) if value.chain_id == "1")
    );
    let intent = Intent {
        id: "intent".into(),
        action_id: "action".into(),
        metadata: None,
    };
    assert!(
        matches!(MessageContent::decode(IntentCodec::encode(intent)?.encode_to_vec())?, MessageContent::Intent(value) if value.action_id == "action")
    );
    let actions = Actions {
        id: "actions".into(),
        description: "desc".into(),
        actions: vec![Action {
            id: "one".into(),
            label: "One".into(),
            image_url: None,
            style: None,
            expires_at: None,
        }],
        expires_at: None,
    };
    assert!(
        matches!(MessageContent::decode(ActionsCodec::encode(actions)?.encode_to_vec())?, MessageContent::Actions(value) if value.id == "actions")
    );
    let update = GroupUpdated {
        initiated_by_inbox_id: "inbox".into(),
        ..Default::default()
    };
    assert!(
        matches!(MessageContent::decode(GroupUpdatedCodec::encode(update)?.encode_to_vec())?, MessageContent::GroupUpdated(value) if value.initiated_by_inbox_id.0 == "inbox")
    );
    let leave = LeaveRequest {
        authenticated_note: Some(b"note".to_vec()),
    };
    assert!(
        matches!(MessageContent::decode(LeaveRequestCodec::encode(leave)?.encode_to_vec())?, MessageContent::LeaveRequest(value) if value.authenticated_note == Some(b"note".to_vec()))
    );
}
