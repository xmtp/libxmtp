use std::sync::Arc;

use xmtp_common::RetryableError;
use xmtp_id::scw_verifier::MultiSmartContractSignatureVerifier;
use xmtp_id::scw_verifier::VerifierError;
use xmtp_proto::{api::IsConnectedCheck, prelude::ApiBuilder, types::AppVersion};

#[derive(Clone)]
pub struct D14nClient<C, G, Store> {
    pub(super) message_client: C,
    pub(super) gateway_client: G,
    pub(super) cursor_store: Store,
    pub(super) scw_verifier: Arc<MultiSmartContractSignatureVerifier>,
}

impl<C, G, Store> D14nClient<C, G, Store> {
    pub fn new(
        message_client: C,
        gateway_client: G,
        cursor_store: Store,
    ) -> Result<Self, VerifierError> {
        Ok(Self {
            message_client,
            gateway_client,
            cursor_store,
            scw_verifier: Arc::new(MultiSmartContractSignatureVerifier::new_from_env()?),
        })
    }
}

#[cfg_attr(any(test, feature = "test-utils"), derive(Clone))]
pub struct D14nClientBuilder<Builder1, Builder2, Store> {
    message_client: Builder1,
    gateway_client: Builder2,
    cursor_store: Store,
}

impl<Builder1, Builder2, Store> D14nClientBuilder<Builder1, Builder2, Store> {
    pub fn new(message_client: Builder1, gateway_client: Builder2, cursor_store: Store) -> Self {
        Self {
            message_client,
            gateway_client,
            cursor_store,
        }
    }

    pub fn cursor_store(&mut self, store: Store) -> &mut Self {
        self.cursor_store = store;
        self
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

impl<Builder1, Builder2, Store> ApiBuilder for D14nClientBuilder<Builder1, Builder2, Store>
where
    Builder1: ApiBuilder<Error = <Builder2 as ApiBuilder>::Error>,
    Builder2: ApiBuilder,
{
    type Output =
        D14nClient<<Builder1 as ApiBuilder>::Output, <Builder2 as ApiBuilder>::Output, Store>;

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
            cursor_store: self.cursor_store,
            scw_verifier: Arc::new(MultiSmartContractSignatureVerifier::new_from_env()?),
        })
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, G, Store> IsConnectedCheck for D14nClient<C, G, Store>
where
    G: IsConnectedCheck + Send + Sync,
    C: IsConnectedCheck + Send + Sync,
    Store: Send + Sync,
{
    async fn is_connected(&self) -> bool {
        self.message_client.is_connected().await && self.gateway_client.is_connected().await
    }
}

#[cfg(any(test, feature = "test-utils"))]
mod test {
    #![allow(clippy::unwrap_used)]
    use xmtp_configuration::LOCALHOST;
    use xmtp_proto::{TestApiBuilder, ToxicProxies};

    use crate::protocol::NoCursorStore;

    use super::*;
    impl<Builder1, Builder2> TestApiBuilder for D14nClientBuilder<Builder1, Builder2, NoCursorStore>
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
