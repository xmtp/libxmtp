//! Compatibility layer for d14n and previous xmtp_api crate
mod identity;
mod mls;
mod streams;
mod to_dyn_api;
mod xmtp_query;

use std::sync::Arc;

use arc_swap::ArcSwap;
use xmtp_common::RetryableError;
use xmtp_id::scw_verifier::MultiSmartContractSignatureVerifier;
use xmtp_id::scw_verifier::VerifierError;
use xmtp_proto::{
    api::IsConnectedCheck, api_client::CursorAwareApi, prelude::ApiBuilder, types::AppVersion,
};

use crate::protocol::{CursorStore, InMemoryCursorStore};

#[derive(Clone)]
pub struct D14nClient<C, G> {
    message_client: C,
    gateway_client: G,
    cursor_store: Arc<ArcSwap<Arc<dyn CursorStore>>>,
    scw_verifier: Arc<MultiSmartContractSignatureVerifier>,
}

impl<C, G> D14nClient<C, G> {
    pub fn new(
        message_client: C,
        gateway_client: G,
        cursor_store: Arc<dyn CursorStore>,
    ) -> Result<Self, VerifierError> {
        Ok(Self {
            message_client,
            gateway_client,
            cursor_store: Arc::new(ArcSwap::from_pointee(cursor_store)),
            scw_verifier: Arc::new(MultiSmartContractSignatureVerifier::new_from_env()?),
        })
    }

    pub fn new_stateless(message_client: C, gateway_client: G) -> Result<Self, VerifierError> {
        Ok(Self {
            message_client,
            gateway_client,
            cursor_store: Arc::new(ArcSwap::from_pointee(
                Arc::new(InMemoryCursorStore::new()) as Arc<_>
            )),
            scw_verifier: Arc::new(MultiSmartContractSignatureVerifier::new_from_env()?),
        })
    }
}

pub struct D14nClientBuilder<Builder1, Builder2> {
    message_client: Builder1,
    gateway_client: Builder2,
    store: Arc<ArcSwap<Arc<dyn CursorStore>>>,
}

impl<Builder1, Builder2> D14nClientBuilder<Builder1, Builder2> {
    pub fn new(
        message_client: Builder1,
        gateway_client: Builder2,
        store: Arc<dyn CursorStore>,
    ) -> Self {
        Self {
            message_client,
            gateway_client,
            store: Arc::new(ArcSwap::from_pointee(store)),
        }
    }

    pub fn new_stateless(message_client: Builder1, gateway_client: Builder2) -> Self {
        Self {
            message_client,
            gateway_client,
            store: Arc::new(ArcSwap::from_pointee(
                Arc::new(InMemoryCursorStore::new()) as Arc<_>
            )),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum D14nBuilderError<E> {
    #[error(transparent)]
    ClientError(E),
    #[error(transparent)]
    VerifierError(#[from] VerifierError),
}

impl<E: RetryableError> RetryableError for D14nBuilderError<E> {
    fn is_retryable(&self) -> bool {
        match self {
            Self::ClientError(e) => e.is_retryable(),
            Self::VerifierError(v) => v.is_retryable(),
        }
    }
}

impl<E> D14nBuilderError<E> {
    fn err(e: E) -> Self {
        Self::ClientError(e)
    }
}

impl<Builder1, Builder2> ApiBuilder for D14nClientBuilder<Builder1, Builder2>
where
    Builder1: ApiBuilder<Error = <Builder2 as ApiBuilder>::Error>,
    Builder2: ApiBuilder,
{
    type Output = D14nClient<<Builder1 as ApiBuilder>::Output, <Builder2 as ApiBuilder>::Output>;

    type Error = D14nBuilderError<<Builder1 as ApiBuilder>::Error>;

    fn set_libxmtp_version(&mut self, version: String) -> Result<(), Self::Error> {
        <Builder1 as ApiBuilder>::set_libxmtp_version(&mut self.message_client, version.clone())
            .map_err(D14nBuilderError::err)?;
        <Builder2 as ApiBuilder>::set_libxmtp_version(&mut self.gateway_client, version)
            .map_err(D14nBuilderError::err)
    }

    fn set_app_version(&mut self, version: AppVersion) -> Result<(), Self::Error> {
        <Builder1 as ApiBuilder>::set_app_version(&mut self.message_client, version.clone())
            .map_err(D14nBuilderError::err)?;
        <Builder2 as ApiBuilder>::set_app_version(&mut self.gateway_client, version)
            .map_err(D14nBuilderError::err)
    }

    fn set_host(&mut self, host: String) {
        <Builder1 as ApiBuilder>::set_host(&mut self.message_client, host);
    }

    fn set_gateway(&mut self, gateway: String) {
        <Builder2 as ApiBuilder>::set_host(&mut self.gateway_client, gateway)
    }

    fn set_tls(&mut self, tls: bool) {
        <Builder1 as ApiBuilder>::set_tls(&mut self.message_client, tls);
        <Builder2 as ApiBuilder>::set_tls(&mut self.gateway_client, tls)
    }

    fn set_retry(&mut self, retry: xmtp_common::Retry) {
        <Builder1 as ApiBuilder>::set_retry(&mut self.message_client, retry.clone());
        <Builder2 as ApiBuilder>::set_retry(&mut self.gateway_client, retry)
    }

    fn rate_per_minute(&mut self, limit: u32) {
        <Builder1 as ApiBuilder>::rate_per_minute(&mut self.message_client, limit);
        <Builder2 as ApiBuilder>::rate_per_minute(&mut self.gateway_client, limit)
    }

    fn port(&self) -> Result<Option<String>, Self::Error> {
        <Builder1 as ApiBuilder>::port(&self.message_client).map_err(D14nBuilderError::err)
    }

    fn host(&self) -> Option<&str> {
        <Builder1 as ApiBuilder>::host(&self.message_client)
    }

    fn build(self) -> Result<Self::Output, Self::Error> {
        Ok(D14nClient {
            message_client: <Builder1 as ApiBuilder>::build(self.message_client)
                .map_err(D14nBuilderError::err)?,
            gateway_client: <Builder2 as ApiBuilder>::build(self.gateway_client)
                .map_err(D14nBuilderError::err)?,
            cursor_store: self.store,
            scw_verifier: Arc::new(MultiSmartContractSignatureVerifier::new_from_env()?),
        })
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, G> IsConnectedCheck for D14nClient<C, G>
where
    G: IsConnectedCheck + Send + Sync,
    C: IsConnectedCheck + Send + Sync,
{
    async fn is_connected(&self) -> bool {
        self.message_client.is_connected().await && self.gateway_client.is_connected().await
    }
}

impl<C1, C2> CursorAwareApi for D14nClient<C1, C2> {
    type CursorStore = Arc<dyn CursorStore>;
    fn set_cursor_store(&self, store: Self::CursorStore) {
        self.cursor_store.store(store.into());
    }
}

impl<B1, B2> CursorAwareApi for D14nClientBuilder<B1, B2> {
    type CursorStore = Arc<dyn CursorStore>;

    fn set_cursor_store(&self, store: Self::CursorStore) {
        self.store.store(store.into());
    }
}

#[cfg(any(test, feature = "test-utils"))]
mod test {
    use xmtp_configuration::LOCALHOST;
    use xmtp_proto::{TestApiBuilder, ToxicProxies};

    use super::*;
    impl<Builder1, Builder2> TestApiBuilder for D14nClientBuilder<Builder1, Builder2>
    where
        Builder1: ApiBuilder<Error = <Builder2 as ApiBuilder>::Error> + 'static,
        Builder2: ApiBuilder + 'static,
        <Builder1 as ApiBuilder>::Output: xmtp_proto::api::Client + 'static,
        <Builder2 as ApiBuilder>::Output: xmtp_proto::api::Client + 'static,
    {
        async fn with_toxiproxy(&mut self) -> ToxicProxies {
            let xmtpd_host = <Builder1 as ApiBuilder>::host(&self.message_client).unwrap();
            let gateway_host = <Builder2 as ApiBuilder>::host(&self.gateway_client).unwrap();
            let proxies = xmtp_proto::init_toxi(&[xmtpd_host, gateway_host]).await;
            <Builder1 as ApiBuilder>::set_host(
                &mut self.message_client,
                format!("{LOCALHOST}:{}", proxies.ports()[0]),
            );
            <Builder2 as ApiBuilder>::set_host(
                &mut self.gateway_client,
                format!("{LOCALHOST}:{}", proxies.ports()[1]),
            );
            proxies
        }
    }
}
