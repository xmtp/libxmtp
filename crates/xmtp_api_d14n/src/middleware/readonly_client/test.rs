#![allow(clippy::unwrap_used)]

use super::*;
use xmtp_proto::api_client::{ToxicProxies, ToxicTestClient};
use xmtp_proto::prelude::ApiBuilder;

#[xmtp_common::async_trait]
impl<C> ToxicTestClient for ReadonlyClient<C>
where
    C: ToxicTestClient,
{
    async fn proxies() -> ToxicProxies {
        <C as ToxicTestClient>::proxies().await
    }
}

impl<B> ApiBuilder for ReadonlyClientBuilder<B>
where
    B: ApiBuilder,
{
    type Output = ReadonlyClient<B::Output>;
    type Error = ();
    fn build(self) -> Result<Self::Output, Self::Error> {
        Ok(self.build_builder().unwrap())
    }
}
