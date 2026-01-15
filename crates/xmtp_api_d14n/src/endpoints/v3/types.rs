//! V3-Specific Types

use prost::{
    Message,
    encoding::{DecodeContext, WireType},
};
use xmtp_proto::xmtp::mls::api::v1::{
    GroupMessage as ProtoGroupMessage, WelcomeMessage as ProtoWelcomeMessage,
};

use crate::protocol::{
    EnvelopeError, Extractor, ProtocolEnvelope, V3GroupMessageExtractor, V3WelcomeMessageExtractor,
};

/// intermediary type to indicate this group message is V3-only
/// Conversions make assumptions about OriginatorID/sequenceID since this message is
/// indicated to only come from V3
#[derive(Default)]
pub struct V3ProtoGroupMessage {
    inner: ProtoGroupMessage,
}

impl TryFrom<V3ProtoGroupMessage> for xmtp_proto::types::GroupMessage {
    type Error = crate::protocol::traits::EnvelopeError;

    fn try_from(value: V3ProtoGroupMessage) -> Result<Self, Self::Error> {
        let mut extractor = V3GroupMessageExtractor::default();
        value.inner.accept(&mut extractor)?;
        extractor
            .get()?
            .ok_or(EnvelopeError::NotFound("v3 message"))
    }
}

impl Message for V3ProtoGroupMessage {
    fn encode_raw(&self, buf: &mut impl prost::bytes::BufMut)
    where
        Self: Sized,
    {
        self.inner.encode_raw(buf)
    }

    fn merge_field(
        &mut self,
        tag: u32,
        wire_type: WireType,
        buf: &mut impl hyper::body::Buf,
        ctx: DecodeContext,
    ) -> Result<(), prost::DecodeError>
    where
        Self: Sized,
    {
        self.inner.merge_field(tag, wire_type, buf, ctx)
    }

    fn encoded_len(&self) -> usize {
        self.inner.encoded_len()
    }

    fn clear(&mut self) {
        self.inner.clear()
    }
}

/// intermediary type to indicate this welcome message is V3-only
#[derive(Default)]
pub struct V3ProtoWelcomeMessage {
    inner: ProtoWelcomeMessage,
}

impl TryFrom<V3ProtoWelcomeMessage> for xmtp_proto::types::WelcomeMessage {
    type Error = crate::protocol::traits::EnvelopeError;

    fn try_from(value: V3ProtoWelcomeMessage) -> Result<Self, Self::Error> {
        let mut extractor = V3WelcomeMessageExtractor::default();
        value.inner.accept(&mut extractor)?;
        Ok(extractor.get()?)
    }
}

impl Message for V3ProtoWelcomeMessage {
    fn encode_raw(&self, buf: &mut impl prost::bytes::BufMut)
    where
        Self: Sized,
    {
        self.inner.encode_raw(buf)
    }

    fn merge_field(
        &mut self,
        tag: u32,
        wire_type: WireType,
        buf: &mut impl hyper::body::Buf,
        ctx: DecodeContext,
    ) -> Result<(), prost::DecodeError>
    where
        Self: Sized,
    {
        self.inner.merge_field(tag, wire_type, buf, ctx)
    }

    fn encoded_len(&self) -> usize {
        self.inner.encoded_len()
    }

    fn clear(&mut self) {
        self.inner.clear()
    }
}
