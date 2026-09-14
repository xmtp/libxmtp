//! The API client object and connection helpers.

use super::*;

use crate::FfiError;
pub use crate::inbox_owner::SigningError;
use crate::logger::init_logger;
use futures::future::try_join_all;

use std::sync::Arc;
use xmtp_api::{ApiClientWrapper, strategies};
use xmtp_api_backend::MessageBackendBuilder;

use xmtp_db::NativeDb;

use xmtp_db::EncryptedMessageStore;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_mls::client::inbox_addresses_with_verifier;

use xmtp_mls::identity_updates::get_creation_signature_kind;
use xmtp_mls::{client::Client as MlsClient, groups::MlsGroup};

use xmtp_proto::api::IsConnectedCheck;

pub type RustXmtpClient = MlsClient<xmtp_mls::MlsContext>;
pub type RustMlsGroup = MlsGroup<xmtp_mls::MlsContext>;

/// the opaque Xmtp Api Client for iOS/Android bindings
#[derive(uniffi::Object, Clone)]
pub struct XmtpApiClient {
    pub(crate) wrapper: ApiClientWrapper<xmtp_mls::XmtpApiClient>,
    pub(crate) api_client: xmtp_mls::XmtpApiClient,
    cache_key: String,
}

impl XmtpApiClient {
    pub(crate) fn inner(&self) -> ApiClientWrapper<xmtp_mls::XmtpApiClient> {
        self.wrapper.clone()
    }
}

#[uniffi::export]
impl XmtpApiClient {
    /// Key for an SDK cache of API clients.
    pub fn cache_key(&self) -> String {
        self.cache_key.clone()
    }
}

/// Connect to the backend at the supplied URL.
#[uniffi::export(async_runtime = "tokio")]
#[xmtp_common::err_span]
pub async fn connect_to_backend(
    backend_url: String,
    client_mode: Option<FfiClientMode>,
    app_version: Option<String>,
    auth_callback: Option<Arc<dyn auth::FfiAuthCallback>>,
    auth_handle: Option<Arc<auth::FfiAuthHandle>>,
) -> Result<Arc<XmtpApiClient>, FfiError> {
    init_logger();
    xmtp_cryptography::install_crypto_provider();
    let app_version = app_version.unwrap_or_default();
    let cache_key = format!("{backend_url}|{app_version}");
    let api_client = MessageBackendBuilder::default()
        .host(&backend_url)
        .app_version(app_version)
        .maybe_auth_callback(
            auth_callback.map(|callback| Arc::new(auth::FfiAuthCallbackBridge::new(callback)) as _),
        )
        .readonly(matches!(
            client_mode.unwrap_or_default(),
            FfiClientMode::Notification
        ))
        .maybe_auth_handle(auth_handle.map(|handle| handle.as_ref().clone().into()))
        .build()?;
    let wrapper = ApiClientWrapper::new(api_client.clone(), strategies::exponential_cooldown());
    Ok(Arc::new(XmtpApiClient {
        wrapper,
        api_client,
        cache_key,
    }))
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn is_connected(api: Arc<XmtpApiClient>) -> bool {
    api.wrapper.api_client.is_connected().await
}

/// Suspend all shared bidi wires in this process until [`resume_streams`].
/// Keep subscriptions and durable progress. New wires also start suspended.
#[uniffi::export(async_runtime = "tokio")]
pub async fn suspend_streams() -> Result<(), FfiError> {
    xmtp_mls::subscriptions::router_callbacks::suspend_bidi_streams().await?;
    Ok(())
}

/// Resume all shared bidi wires; return before reconnect and processing complete.
/// Use [`FfiXmtpClient::catch_up_to_live`] to wait for bounded fixed-target processing.
#[uniffi::export(async_runtime = "tokio")]
pub async fn resume_streams() -> Result<(), FfiError> {
    xmtp_mls::subscriptions::router_callbacks::resume_bidi_streams().await?;
    Ok(())
}

/**
 * Static Get the inbox state for each `inbox_id`.
 */
#[uniffi::export(async_runtime = "tokio")]
#[tracing::instrument(level = "debug", skip_all)]
pub async fn inbox_state_from_inbox_ids(
    api: Arc<XmtpApiClient>,
    inbox_ids: Vec<String>,
) -> Result<Vec<FfiInboxState>, FfiError> {
    let scw_verifier = Arc::new(Box::new(api.inner()) as Box<dyn SmartContractSignatureVerifier>);

    let db = NativeDb::builder().ephemeral().build_unencrypted()?;
    let store = EncryptedMessageStore::new(db)?;

    let states = inbox_addresses_with_verifier(
        &api.wrapper,
        &store.db(),
        inbox_ids.iter().map(String::as_str).collect(),
        &scw_verifier,
    )
    .await?;

    let mapped_futures = states.into_iter().map(|state| async {
        // TODO: Implement this field as part of the core association state.
        // https://github.com/xmtp/libxmtp/issues/2583
        let signature_kind =
            get_creation_signature_kind(&store.db(), scw_verifier.clone(), state.inbox_id())
                .await?;

        let mut ffi_state: FfiInboxState = state.into();
        ffi_state.creation_signature_kind = signature_kind.map(Into::into);

        Ok::<FfiInboxState, FfiError>(ffi_state)
    });

    try_join_all(mapped_futures).await
}
