use xmtp_common::{MaybeSend, MaybeSync};
use xmtp_proto::api::IsConnectedCheck;

use crate::MigrationClient;

#[xmtp_common::async_trait]
impl<V3, D14n, Store> IsConnectedCheck for MigrationClient<V3, D14n, Store>
where
    V3: IsConnectedCheck,
    D14n: IsConnectedCheck,
    Store: MaybeSend + MaybeSync,
{
    async fn is_connected(&self) -> bool {
        self.v3_client.is_connected().await && self.xmtpd_client.is_connected().await
    }
}
