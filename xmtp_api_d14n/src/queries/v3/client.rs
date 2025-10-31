use xmtp_proto::api::IsConnectedCheck;
use xmtp_proto::prelude::ApiBuilder;
use xmtp_proto::types::AppVersion;

#[derive(Clone)]
pub struct V3Client<C, Store> {
    pub(super) client: C,
    pub(super) cursor_store: Store,
}

impl<C, Store> V3Client<C, Store> {
    pub fn new(client: C, cursor_store: Store) -> Self {
        Self {
            client,
            cursor_store,
        }
    }

    pub fn client_mut(&mut self) -> &mut C {
        &mut self.client
    }
}

pub struct V3ClientBuilder<Builder, Store> {
    client: Builder,
    store: Store,
}

impl<Builder, Store> V3ClientBuilder<Builder, Store> {
    pub fn new(client: Builder, store: Store) -> Self {
        Self { client, store }
    }
}

impl<Builder, Store> ApiBuilder for V3ClientBuilder<Builder, Store>
where
    Builder: ApiBuilder,
{
    type Output = V3Client<<Builder as ApiBuilder>::Output, Store>;

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

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C, Store> IsConnectedCheck for V3Client<C, Store>
where
    C: IsConnectedCheck + Send + Sync,
    Store: Send + Sync,
{
    async fn is_connected(&self) -> bool {
        self.client.is_connected().await
    }
}
