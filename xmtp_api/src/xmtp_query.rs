use crate::ApiClientWrapper;
use xmtp_api_d14n::protocol::{XmtpEnvelope, XmtpQuery};
use xmtp_proto::types::{GlobalCursor, Topic};

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C> XmtpQuery for ApiClientWrapper<C>
where
    C: XmtpQuery,
{
    type Error = <C as XmtpQuery>::Error;

    async fn query_at(
        &self,
        topic: Topic,
        at: Option<GlobalCursor>,
    ) -> Result<XmtpEnvelope, Self::Error> {
        <C as XmtpQuery>::query_at(&self.api_client, topic, at).await
    }
}
