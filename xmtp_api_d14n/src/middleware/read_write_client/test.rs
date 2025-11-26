#![allow(clippy::unwrap_used)]

use super::*;
use xmtp_proto::api_client::{ToxicProxies, ToxicTestClient};
use xmtp_proto::prelude::ApiBuilder;

#[xmtp_common::async_trait]
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
    fn build(self) -> Result<Self::Output, Self::Error> {
        let c = self.build_builder().unwrap();
        Ok(c)
    }
}
