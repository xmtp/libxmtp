use std::sync::Arc;

use xmtp_proto::types::{GlobalCursor, Topic};

use crate::protocol::{XmtpEnvelope, XmtpQuery};

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T> XmtpQuery for Box<T>
where
    T: XmtpQuery + ?Sized,
{
    type Error = T::Error;

    /// Query every [`Topic`] at [`GlobalCursor`]
    async fn query_at(
        &self,
        topic: Topic,
        at: Option<GlobalCursor>,
    ) -> Result<XmtpEnvelope, Self::Error> {
        <T as XmtpQuery>::query_at(&**self, topic, at).await
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T> XmtpQuery for Arc<T>
where
    T: XmtpQuery + ?Sized,
{
    type Error = T::Error;

    /// Query every [`Topic`] at [`GlobalCursor`]
    async fn query_at(
        &self,
        topic: Topic,
        at: Option<GlobalCursor>,
    ) -> Result<XmtpEnvelope, Self::Error> {
        <T as XmtpQuery>::query_at(&**self, topic, at).await
    }
}
