use xmtp_proto::api::IsConnectedCheck;
use xmtp_proto::prelude::ApiBuilder;
use xmtp_proto::types::AppVersion;

mod identity;
mod mls;
mod streams;
mod to_dyn_api;

#[derive(Clone)]
pub struct V3Client<C> {
    client: C,
}

impl<C> V3Client<C> {
    pub fn new(client: C) -> Self {
        Self { client }
    }

    pub fn builder<T: Default>() -> V3ClientBuilder<T> {
        V3ClientBuilder::new(T::default())
    }

    pub fn client_mut(&mut self) -> &mut C {
        &mut self.client
    }
}

pub struct V3ClientBuilder<Builder> {
    client: Builder,
}

impl<Builder> V3ClientBuilder<Builder> {
    pub fn new(client: Builder) -> Self {
        Self { client }
    }
}

impl<Builder> ApiBuilder for V3ClientBuilder<Builder>
where
    Builder: ApiBuilder,
    <Builder as ApiBuilder>::Output: xmtp_proto::api::Client,
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
        Ok(V3Client::new(<Builder as ApiBuilder>::build(self.client)?))
    }

    fn set_retry(&mut self, retry: xmtp_common::Retry) {
        <Builder as ApiBuilder>::set_retry(&mut self.client, retry)
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
