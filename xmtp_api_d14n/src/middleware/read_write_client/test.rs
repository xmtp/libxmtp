#![allow(clippy::unwrap_used)]

use super::*;
use xmtp_proto::api_client::{ToxicProxies, ToxicTestClient};
use xmtp_proto::prelude::ApiBuilder;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<R, W> ToxicTestClient for ReadWriteClient<R, W>
where
    R: ToxicTestClient,
    W: ToxicTestClient,
{
    async fn proxies() -> ToxicProxies {
        let mut base = <R as ToxicTestClient>::proxies().await;
        base.merge(<W as ToxicTestClient>::proxies().await);
        base
    }
}

impl<BRead, BWrite> ApiBuilder for ReadWriteClientBuilder<BRead, BWrite>
where
    BRead: ApiBuilder,
    BWrite: ApiBuilder,
{
    type Output = ReadWriteClient<BRead::Output, BWrite::Output>;
    type Error = ();
    fn set_libxmtp_version(&mut self, _: String) -> Result<(), Self::Error> {
        unimplemented!("no way to set host for a client that needs 2 hosts")
    }

    fn set_app_version(&mut self, _: xmtp_proto::types::AppVersion) -> Result<(), Self::Error> {
        unimplemented!("no way to set host for a client that needs 2 hosts")
    }

    fn set_host(&mut self, _host: String) {
        unimplemented!("no way to set host for a client that needs 2 hosts")
    }

    fn set_tls(&mut self, _: bool) {
        unimplemented!("no way to set host for a client that needs 2 hosts")
    }

    fn rate_per_minute(&mut self, _: u32) {
        unimplemented!("no way to set host for a client that needs 2 hosts")
    }

    fn port(&self) -> Result<Option<String>, Self::Error> {
        unimplemented!("no way to set host for a client that needs 2 hosts")
    }

    fn host(&self) -> Option<&str> {
        unimplemented!("no way to set host for a client that needs 2 hosts")
    }

    fn build(self) -> Result<Self::Output, Self::Error> {
        let c = self.build_builder().unwrap();
        Ok(c)
    }

    fn set_retry(&mut self, _: xmtp_common::Retry) {
        unimplemented!("no way to set host for a client that needs 2 hosts")
    }
}
