//! A selected-group consumer over committed local messages.

#[cfg(any(test, feature = "test-utils"))]
pub mod stream_stats;

use super::{
    Result,
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
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_proto::{api_client::XmtpMlsStreams, types::GroupId};

pub struct StreamGroupMessages {
    inner: BoxDynStream<'static, Result<StoredGroupMessage>>,
}

impl StreamGroupMessages {
    pub async fn new<C: XmtpSharedContext + 'static>(
        context: &C,
        groups: Vec<GroupId>,
    ) -> Result<Self>
    where
        C::ApiClient: XmtpMlsStreams,
    {
        Self::new_owned(context.clone(), groups).await
    }

    pub async fn new_owned<C: XmtpSharedContext + 'static>(
        context: C,
        groups: Vec<GroupId>,
    ) -> Result<Self>
    where
        C::ApiClient: XmtpMlsStreams,
    {
        let reader = MessageReader::new(
            context,
            DeliveryScope::Groups(groups),
            LocalDeliveryFilter::default(),
            None,
        )?;
        Ok(Self {
            inner: Box::pin(reader.into_stream()),
        })
    }
}

impl Stream for StreamGroupMessages {
    type Item = Result<StoredGroupMessage>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}
