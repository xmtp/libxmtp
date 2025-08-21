//! V3-Specific Types

use prost::{Message, encoding::{WireType, DecodeContext}};
use xmtp_proto::{mls_v1::group_message, xmtp::mls::api::v1::{GroupMessage as ProtoGroupMessage, WelcomeMessage as ProtoWelcomeMessage}, ConversionError};


/// intermediary type to indicate this group message is V3-only
/// Conversions make assumptions about OriginatorID/sequenceID since this message is
/// indicated to only come from V3
#[derive(Default)]
pub struct V3ProtoGroupMessage {
    inner: ProtoGroupMessage
}

impl TryFrom<V3ProtoGroupMessage> for xmtp_proto::types::GroupMessage {
    type Error = xmtp_proto::ConversionError;

    fn try_from(value: V3ProtoGroupMessage) -> Result<Self, Self::Error> {
        let Some(group_message::Version::V1(v1)) = value.inner.version else {
            return Err(ConversionError::InvalidVersion)
        };
        todo!()
        /*
        * TODO: Need to figure out whether this is a commit or an application message
        Ok(GroupMessage {
            cursor: Cursor {
                sequence_id: v1.id,
                originator_id: xmtp_configuration::
            }
        })
    */
    }
}

impl Message for V3ProtoGroupMessage {
    fn encode_raw(&self, buf: &mut impl prost::bytes::BufMut)
    where
        Self: Sized {
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
        Self: Sized {
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
    inner: ProtoWelcomeMessage
}

impl Message for V3ProtoWelcomeMessage {
    fn encode_raw(&self, buf: &mut impl prost::bytes::BufMut)
    where
        Self: Sized {
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
        Self: Sized {
        self.inner.merge_field(tag, wire_type, buf, ctx)
    }

    fn encoded_len(&self) -> usize {
        self.inner.encoded_len()
    }

    fn clear(&mut self) {
        self.inner.clear()
    }
}
