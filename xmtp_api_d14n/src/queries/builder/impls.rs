use std::sync::Arc;

use xmtp_proto::types::{GlobalCursor, Topic};

use crate::protocol::{XmtpEnvelope, XmtpQuery};

#[xmtp_common::async_trait]
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

#[xmtp_common::async_trait]
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
