use super::*;

use std::collections::HashMap;

use alloy::{
    dyn_abi::SolType,
    primitives::{U256, eip191_hash_message},
    providers::{DynProvider, Provider},
    signers::Signer as _,
};
use xmtp_id::utils::test::{
    CoinbaseSmartWallet::CoinbaseSmartWalletInstance, SignatureWithNonce, SmartWalletContext,
    docker_smart_wallet,
};

/// A Coinbase smart wallet deployed on the local anvil chain, signing through
/// one of its owners. The backend verifies its ERC-1271 signatures on chain.
struct ContractWalletSigner {
    owner: PrivateKeySigner,
    wallet: CoinbaseSmartWalletInstance<DynProvider>,
    chain_id: u64,
}

impl ContractWalletSigner {
    fn address(&self) -> String {
        self.wallet.address().to_string().to_lowercase()
    }
}

#[xmtp_common::async_trait]
impl Signer for ContractWalletSigner {
    async fn identity(&self) -> Result<PublicIdentity, SignerError> {
        Ok(PublicIdentity {
            identifier: self.address(),
            kind: PublicIdentityKind::Ethereum,
        })
    }

    async fn kind(&self) -> Result<SignerKind, SignerError> {
        Ok(SignerKind::Scw {
            chain_id: self.chain_id,
            block_number: None,
        })
    }

    async fn sign(&self, request: SigningRequest) -> Result<Signature, SignerError> {
        let hash = eip191_hash_message(request.text);
        let replay_safe = self
            .wallet
            .replaySafeHash(hash)
            .call()
            .await
            .map_err(|_| SignerError::Failed)?;
        let signed = self
            .owner
            .sign_hash(&replay_safe)
            .await
            .map_err(|_| SignerError::Failed)?;
        Ok(Signature::Scw {
            bytes: SignatureWithNonce::abi_encode(&(U256::from(0), signed.as_bytes().to_vec())),
            address: self.address(),
            chain_id: self.chain_id,
            block_number: None,
        })
    }
}

fn text_of(message: &crate::Message) -> Option<&str> {
    match &message.0.content {
        MessageContent::Text(text) => Some(text),
        _ => None,
    }
}

// A smart contract wallet creates a client through the public signer
// callback. The client is built again from storage, checks reachability both
// ways, and sends a group message that an EOA member receives.
#[xmtp_common::test(unwrap_try = true)]
async fn smart_contract_wallet_signer_creates_builds_and_messages() {
    let SmartWalletContext {
        factory,
        sw,
        owner0,
        ..
    } = docker_smart_wallet::default().await;
    let chain_id = factory.provider().get_chain_id().await?;
    let signer = Arc::new(ContractWalletSigner {
        owner: owner0,
        wallet: sw,
        chain_id,
    });
    let identity = signer.identity().await?;

    let root = temp_root("smart-contract-wallet");
    std::fs::create_dir_all(&root)?;
    let mut settings = options();
    settings.storage.location = explicit_location(&root.join("wallet.db3"));
    let created = Client::create(signer.clone(), settings.clone()).await?;
    let inbox_id = created.inbox_id();
    assert_eq!(created.identity().identifier, identity.identifier);
    created.end().await?;

    let wallet = Client::build(identity.clone(), settings, Some(inbox_id.clone())).await?;
    assert_eq!(wallet.inbox_id(), inbox_id);
    let state = wallet.inbox_state(true).await?;
    assert_eq!(state.recovery_identity.identifier, identity.identifier);
    assert!(matches!(
        state.creation_signature_kind,
        Some(crate::SignatureKind::Erc1271)
    ));

    let peer = Client::create(crate::generate_local_signer().await, options()).await?;
    let unknown = signer::identity(crate::generate_local_signer().await).await?;
    let key =
        |identity: &PublicIdentity| format!("ethereum:{}", identity.identifier.to_lowercase());
    assert_eq!(
        wallet
            .can_message(vec![peer.identity(), unknown.clone()])
            .await?,
        HashMap::from([(key(&peer.identity()), true), (key(&unknown), false)])
    );
    assert_eq!(
        peer.can_message(vec![identity.clone()]).await?,
        HashMap::from([(key(&identity), true)])
    );

    let group = wallet
        .conversations()
        .create_group(vec![peer.inbox_id()], None)
        .await?;
    let sent = group
        .send_text("smart contract wallet message".into(), None)
        .await?;
    peer.conversations().sync_all(None).await?;
    let received = peer
        .conversations()
        .get_message_by_id(sent)
        .await?
        .expect("the peer received the message");
    assert_eq!(text_of(&received), Some("smart contract wallet message"));
    assert_eq!(received.0.sender_inbox_id, inbox_id);

    // The wallet is the recovery identity, so its signature removes an
    // account it added. The backend verifies that signature on chain too.
    let added = crate::generate_local_signer().await;
    let added_identity = signer::identity(added.clone()).await?;
    let has_added = |state: &crate::InboxState| {
        state
            .identities
            .iter()
            .any(|identity| identity.identifier == added_identity.identifier)
    };
    wallet.unsafe_add_account(added, false).await?;
    assert!(has_added(&wallet.inbox_state(true).await?));
    wallet
        .remove_account(signer.clone(), added_identity.clone())
        .await?;
    let state = wallet.inbox_state(true).await?;
    assert!(!has_added(&state));
    assert_eq!(state.recovery_identity.identifier, identity.identifier);

    wallet.end().await?;
    peer.end().await?;
    std::fs::remove_dir_all(root)?;
}

const VERIFY_SCW: &str = "/xmtp.backend.v1.IdentityService/VerifySmartContractWalletSignatures";

/// Forwards every request to the shared backend. While `failing` is set,
/// smart contract wallet verification fails with a retryable transport error
/// and each such request is counted.
struct FlakyVerifierTransport {
    inner: xmtp_proto::api::ArcClient,
    failing: Arc<std::sync::atomic::AtomicBool>,
    failed: Arc<std::sync::atomic::AtomicUsize>,
}

#[xmtp_common::async_trait]
impl xmtp_proto::api::Client for FlakyVerifierTransport {
    fn host(&self) -> &str {
        self.inner.host()
    }

    fn has_credential_source(&self) -> bool {
        self.inner.has_credential_source()
    }

    async fn request(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: prost::bytes::Bytes,
    ) -> Result<http::Response<prost::bytes::Bytes>, xmtp_proto::api::ApiClientError> {
        use std::sync::atomic::Ordering;
        if path.path() == VERIFY_SCW && self.failing.load(Ordering::SeqCst) {
            self.failed.fetch_add(1, Ordering::SeqCst);
            return Err(xmtp_proto::api::ApiClientError::Expired(
                xmtp_common::time::Expired,
            ));
        }
        self.inner.request(request, path, body).await
    }

    async fn stream(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: prost::bytes::Bytes,
    ) -> Result<http::Response<xmtp_proto::api::BytesStream>, xmtp_proto::api::ApiClientError> {
        self.inner.stream(request, path, body).await
    }

    async fn bidi_stream(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: xmtp_common::BoxDynStream<'static, prost::bytes::Bytes>,
    ) -> Result<http::Response<xmtp_proto::api::BytesStream>, xmtp_proto::api::ApiClientError> {
        self.inner.bidi_stream(request, path, body).await
    }
}

#[xmtp_common::async_trait]
impl xmtp_proto::api::IsConnectedCheck for FlakyVerifierTransport {
    async fn is_connected(&self) -> bool {
        self.inner.is_connected().await
    }
}

/// Inbox lookups that need a smart contract wallet proof stay retryable when
/// the verifier fails transiently. The wallet's cached state is dropped and
/// the viewer has none, so every lookup must call the failing verifier.
#[xmtp_common::test(unwrap_try = true)]
async fn inbox_lookups_stay_retryable_on_transient_verifier_failure() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use xmtp_db::ConnectionExt;
    use xmtp_db::diesel::{RunQueryDsl, sql_query};
    use xmtp_proto::api::ToBoxedClient;

    let SmartWalletContext {
        factory,
        sw,
        owner0,
        ..
    } = docker_smart_wallet::default().await;
    let chain_id = factory.provider().get_chain_id().await?;
    let signer = Arc::new(ContractWalletSigner {
        owner: owner0,
        wallet: sw,
        chain_id,
    });
    let identity = signer.identity().await?;

    let failing = Arc::new(AtomicBool::new(false));
    let failed = Arc::new(AtomicUsize::new(0));
    let Some(BackendSource::Options { options: backend }) = options().backend else {
        panic!("test options name backend options");
    };
    let honest = crate::Backend::connect(backend).await?;
    let backend_client = honest.api.inner();
    let transport = FlakyVerifierTransport {
        inner: backend_client.inner().clone(),
        failing: failing.clone(),
        failed: failed.clone(),
    };
    let api = xmtp_api_backend::BackendClient::new(transport.arced())
        .with_auth_handle(backend_client.auth_handle());
    let flaky = ClientOptions {
        backend: Some(BackendSource::Connected {
            backend: Arc::new(crate::Backend {
                api: Arc::new(xmtp_api_backend::TrackedStatsClient::new(api)),
                auth_handle: honest.auth_handle,
                options: honest.options,
            }),
        }),
        ..options()
    };
    let wallet = Client::create(signer.clone(), flaky.clone()).await?;
    let viewer = Client::create(crate::generate_local_signer().await, flaky).await?;
    wallet
        .inner
        .context
        .db()
        .raw_query(|conn| sql_query("DELETE FROM association_state").execute(conn))?;
    failing.store(true, Ordering::SeqCst);

    let mut routes = vec![];
    let before = failed.load(Ordering::SeqCst);
    routes.push((
        "inbox_state",
        wallet.inbox_state(true).await.err(),
        failed.load(Ordering::SeqCst) > before,
    ));
    let before = failed.load(Ordering::SeqCst);
    routes.push((
        "inbox_states",
        viewer
            .inbox_states(vec![wallet.inbox_id()], true)
            .await
            .err(),
        failed.load(Ordering::SeqCst) > before,
    ));
    let before = failed.load(Ordering::SeqCst);
    routes.push((
        "inbox_id_for",
        viewer.inbox_id_for(identity).await.err(),
        failed.load(Ordering::SeqCst) > before,
    ));
    let mut observed = vec![];
    for (route, error, verified) in routes {
        assert!(verified, "{route} did not call the verifier");
        let error = error.unwrap_or_else(|| panic!("{route} succeeded"));
        let (kind, details) = match &error {
            XmtpError::Unknown(details) => ("Unknown", details),
            XmtpError::RequestTimeout(details) => ("RequestTimeout", details),
            other => panic!("{route}: {other:?}"),
        };
        observed.push((
            route,
            kind,
            details.code.clone(),
            format!("{:?}", details.category),
            details.retryable,
        ));
    }
    let unknown = |route| {
        (
            route,
            "Unknown",
            "Unknown".to_owned(),
            "Unknown".to_owned(),
            true,
        )
    };
    assert_eq!(
        observed,
        vec![
            unknown("inbox_state"),
            unknown("inbox_states"),
            unknown("inbox_id_for"),
        ]
    );
    failing.store(false, Ordering::SeqCst);
    viewer.end().await?;
    wallet.end().await?;
}
