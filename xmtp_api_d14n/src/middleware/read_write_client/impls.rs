use super::*;
use xmtp_proto::api::IsConnectedCheck;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<R, W> IsConnectedCheck for ReadWriteClient<R, W>
where
    R: IsConnectedCheck + Send + Sync,
    W: IsConnectedCheck + Send + Sync,
{
    async fn is_connected(&self) -> bool {
        self.read.is_connected().await && self.write.is_connected().await
    }
}
