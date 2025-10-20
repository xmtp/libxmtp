use std::sync::Arc;

use parking_lot::RwLock;
use xmtp_proto::api::IsConnectedCheck;
use xmtp_proto::api_client::CursorAwareApi;
use xmtp_proto::prelude::ApiBuilder;
use xmtp_proto::types::AppVersion;

use crate::protocol::{CursorStore, InMemoryCursorStore};

mod identity;
mod mls;
mod streams;
mod to_dyn_api;
mod xmtp_query;

#[derive(Clone)]
pub struct V3Client<C> {
    client: C,
    cursor_store: Arc<RwLock<Arc<dyn CursorStore>>>,
}

impl<C> V3Client<C> {
    pub fn new(client: C, cursor_store: Arc<dyn CursorStore>) -> Self {
        Self {
            client,
            cursor_store: Arc::new(RwLock::new(cursor_store)),
        }
    }

    pub fn new_stateless(client: C) -> Self {
        Self {
            client,
            cursor_store: Arc::new(RwLock::new(Arc::new(InMemoryCursorStore::new()) as Arc<_>)),
        }
    }

    pub fn builder<T: Default>() -> V3ClientBuilder<T> {
        V3ClientBuilder::new(
            T::default(),
            Arc::new(InMemoryCursorStore::default()) as Arc<_>,
        )
    }

    pub fn client_mut(&mut self) -> &mut C {
        &mut self.client
    }
}

pub struct V3ClientBuilder<Builder> {
    client: Builder,
    store: Arc<RwLock<Arc<dyn CursorStore>>>,
}

impl<Builder> V3ClientBuilder<Builder> {
    pub fn new(client: Builder, store: Arc<dyn CursorStore>) -> Self {
        Self {
            client,
            store: Arc::new(RwLock::new(store)),
        }
    }

    pub fn new_stateless(client: Builder) -> Self {
        Self {
            client,
            store: Arc::new(RwLock::new(Arc::new(InMemoryCursorStore::new()) as Arc<_>)),
        }
    }
}

impl<Builder> ApiBuilder for V3ClientBuilder<Builder>
where
    Builder: ApiBuilder,
{
    type Output = V3Client<<Builder as ApiBuilder>::Output>;

    type Error = <Builder as ApiBuilder>::Error;

    fn set_libxmtp_version(&mut self, version: String) -> Result<(), Self::Error> {
        <Builder as ApiBuilder>::set_libxmtp_version(&mut self.client, version)
    }

    fn set_app_version(&mut self, version: AppVersion) -> Result<(), Self::Error> {
        <Builder as ApiBuilder>::set_app_version(&mut self.client, version)
    }

    fn set_host(&mut self, host: String) {
        <Builder as ApiBuilder>::set_host(&mut self.client, host)
    }

    fn set_tls(&mut self, tls: bool) {
        <Builder as ApiBuilder>::set_tls(&mut self.client, tls)
    }

    fn rate_per_minute(&mut self, limit: u32) {
        <Builder as ApiBuilder>::rate_per_minute(&mut self.client, limit)
    }

    fn port(&self) -> Result<Option<String>, Self::Error> {
        <Builder as ApiBuilder>::port(&self.client)
    }

    fn host(&self) -> Option<&str> {
        <Builder as ApiBuilder>::host(&self.client)
    }

    fn build(self) -> Result<Self::Output, Self::Error> {
        Ok(V3Client {
            client: <Builder as ApiBuilder>::build(self.client)?,
            cursor_store: self.store,
        })
    }

    fn set_retry(&mut self, retry: xmtp_common::Retry) {
        <Builder as ApiBuilder>::set_retry(&mut self.client, retry)
    }
}

impl<C> CursorAwareApi for V3Client<C> {
    type CursorStore = Arc<dyn CursorStore>;
    fn set_cursor_store(&self, store: Self::CursorStore) {
        let mut cstore = self.cursor_store.write();
        *cstore = store;
    }
}

impl<B1: ApiBuilder> CursorAwareApi for V3ClientBuilder<B1> {
    type CursorStore = Arc<dyn CursorStore>;

    fn set_cursor_store(&self, store: Self::CursorStore) {
        let mut cstore = self.store.write();
        *cstore = store;
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C> IsConnectedCheck for V3Client<C>
where
    C: IsConnectedCheck + Send + Sync,
{
    async fn is_connected(&self) -> bool {
        self.client.is_connected().await
    }
}

#[cfg(any(test, feature = "test-utils"))]
mod test {
    #![allow(clippy::unwrap_used)]
    use xmtp_configuration::LOCALHOST;
    use xmtp_proto::{TestApiBuilder, ToxicProxies};

    use super::*;
    impl<Builder> TestApiBuilder for V3ClientBuilder<Builder>
    where
        Builder: ApiBuilder,
        <Builder as ApiBuilder>::Output: xmtp_proto::api::Client,
    {
        async fn with_toxiproxy(&mut self) -> ToxicProxies {
            let host = <Builder as ApiBuilder>::host(&self.client).unwrap();
            let proxies = xmtp_proto::init_toxi(&[host]).await;
            <Builder as ApiBuilder>::set_host(
                &mut self.client,
                format!("{LOCALHOST}:{}", proxies.ports()[0]),
            );
            proxies
        }
    }
}
