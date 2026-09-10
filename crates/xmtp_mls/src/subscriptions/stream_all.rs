//! The default all-group consumer reads committed local messages.

#[cfg(test)]
mod tests;

#[cfg(any(test, feature = "test-utils"))]
use super::message_reader::MessageReaderControl;
use super::{
    Result,
    incoming::IncomingCoordinator,
    local_delivery::{DeliveryScope, LocalDeliveryFilter},
    message_reader::MessageReader,
};
use crate::context::XmtpSharedContext;
use futures::Stream;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use xmtp_common::BoxDynStream;
use xmtp_db::{
    consent_record::ConsentState, group::ConversationType, group_message::StoredGroupMessage,
};
use xmtp_proto::api_client::XmtpMlsStreams;

pub struct StreamAllMessages {
    inner: BoxDynStream<'static, Result<StoredGroupMessage>>,
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) control: MessageReaderControl,
}

impl StreamAllMessages {
    pub async fn new<C: XmtpSharedContext + 'static>(
        context: &C,
        conversation_type: Option<ConversationType>,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<Self>
    where
        C::ApiClient: XmtpMlsStreams,
    {
        Self::new_owned(context.clone(), conversation_type, consent_states).await
    }

    pub async fn new_owned<C: XmtpSharedContext + 'static>(
        context: C,
        conversation_type: Option<ConversationType>,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<Self>
    where
        C::ApiClient: XmtpMlsStreams,
    {
        let _coordinator = IncomingCoordinator::enable_stream_transport(&context);
        let reader = MessageReader::new(
            context,
            DeliveryScope::All,
            LocalDeliveryFilter {
                conversation_type,
                consent_states,
            },
            None,
        )?;
        #[cfg(any(test, feature = "test-utils"))]
        let control = reader.control();
        Ok(Self {
            inner: Box::pin(reader.into_stream()),
            #[cfg(any(test, feature = "test-utils"))]
            control,
        })
    }
}

impl Stream for StreamAllMessages {
    type Item = Result<StoredGroupMessage>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}
