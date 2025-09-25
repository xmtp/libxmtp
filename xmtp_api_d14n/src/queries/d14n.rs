//! Compatibility layer for d14n and previous xmtp_api crate
mod identity;
mod mls;
mod streams;
mod to_dyn_api;

use std::sync::Arc;

use xmtp_proto::{
    api::IsConnectedCheck, api_client::CursorAwareApi, prelude::ApiBuilder, types::AppVersion,
};

use crate::protocol::{CursorStore, InMemoryCursorStore};

#[derive(Clone)]
pub struct D14nClient<C, G> {
    message_client: C,
    gateway_client: G,
    cursor_store: Arc<dyn CursorStore>,
}

impl<C, G> D14nClient<C, G> {
    pub fn new(message_client: C, gateway_client: G, cursor_store: Arc<dyn CursorStore>) -> Self {
        Self {
            message_client,
            gateway_client,
            cursor_store,
        }
    }
}

pub struct D14nClientBuilder<Builder1, Builder2> {
    message_client: Builder1,
    gateway_client: Builder2,
    store: Arc<dyn CursorStore>,
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
            store,
        }
    }

    pub fn new_stateless(message_client: Builder1, gateway_client: Builder2) -> Self {
        Self {
            message_client,
            gateway_client,
            store: Arc::new(InMemoryCursorStore::new()) as Arc<_>,
        }
    }
}

impl<Builder1, Builder2> ApiBuilder for D14nClientBuilder<Builder1, Builder2>
where
    Builder1: ApiBuilder<Error = <Builder2 as ApiBuilder>::Error>,
    Builder2: ApiBuilder,
{
    type Output = D14nClient<<Builder1 as ApiBuilder>::Output, <Builder2 as ApiBuilder>::Output>;

    type Error = <Builder1 as ApiBuilder>::Error;

    fn set_libxmtp_version(&mut self, version: String) -> Result<(), Self::Error> {
        <Builder1 as ApiBuilder>::set_libxmtp_version(&mut self.message_client, version.clone())?;
        <Builder2 as ApiBuilder>::set_libxmtp_version(&mut self.gateway_client, version)
    }

    fn set_app_version(&mut self, version: AppVersion) -> Result<(), Self::Error> {
        <Builder1 as ApiBuilder>::set_app_version(&mut self.message_client, version.clone())?;
        <Builder2 as ApiBuilder>::set_app_version(&mut self.gateway_client, version)
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
        <Builder1 as ApiBuilder>::port(&self.message_client)
    }

    fn host(&self) -> Option<&str> {
        <Builder1 as ApiBuilder>::host(&self.message_client)
    }

    fn build(self) -> Result<Self::Output, Self::Error> {
        Ok(D14nClient::new(
            <Builder1 as ApiBuilder>::build(self.message_client)?,
            <Builder2 as ApiBuilder>::build(self.gateway_client)?,
            self.store,
        ))
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
    fn set_cursor_store(&mut self, store: Self::CursorStore) {
        self.cursor_store = store;
    }
}

impl<B1, B2> CursorAwareApi for D14nClientBuilder<B1, B2> {
    type CursorStore = Arc<dyn CursorStore>;

    fn set_cursor_store(&mut self, store: Self::CursorStore) {
        self.store = store;
    }
}

#[cfg(any(test, feature = "test-utils"))]
mod test {
    use xmtp_configuration::LOCALHOST;
    use xmtp_proto::{TestApiBuilder, ToxicProxies};

    use super::*;
    impl<Builder1, Builder2> TestApiBuilder for D14nClientBuilder<Builder1, Builder2>
    where
        Builder1: ApiBuilder<Error = <Builder2 as ApiBuilder>::Error>,
        Builder2: ApiBuilder,
        <Builder1 as ApiBuilder>::Output: xmtp_proto::api::Client,
        <Builder2 as ApiBuilder>::Output: xmtp_proto::api::Client,
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
