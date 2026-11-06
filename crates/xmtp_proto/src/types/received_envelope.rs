//! Receive-only response views that retain each `ServerEnvelope` as received.
//! Decoding a generated struct drops unknown fields; these views keep the bytes.
use crate::backend_v1 as wire;
use bytes::Bytes;
use prost::Message;

/// One backend envelope: its exact received bytes and decoded metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct ReceivedEnvelope {
    meta: Option<wire::EnvelopeMeta>,
    raw: Bytes,
}

impl ReceivedEnvelope {
    /// Decode the complete envelope to validate it, and keep its bytes unchanged.
    /// Copy them so a retained envelope does not keep its whole response frame alive.
    pub fn from_raw(raw: Bytes) -> Result<Self, prost::DecodeError> {
        let decoded = wire::ServerEnvelope::decode(raw.as_ref())?;
        Ok(Self {
            meta: decoded.meta,
            raw: Bytes::copy_from_slice(&raw),
        })
    }

    pub fn meta(&self) -> Option<&wire::EnvelopeMeta> {
        self.meta.as_ref()
    }

    /// The serialized `ServerEnvelope` exactly as the backend sent it.
    pub fn raw(&self) -> &Bytes {
        &self.raw
    }

    /// Decode the complete envelope for processing. Unknown fields are dropped.
    pub fn decode(&self) -> Result<wire::ServerEnvelope, crate::ConversionError> {
        Ok(wire::ServerEnvelope::decode(self.raw.clone())?)
    }
}

/// Decode a repeated raw envelope field.
pub fn received_envelopes(raw: Vec<Bytes>) -> Result<Vec<ReceivedEnvelope>, prost::DecodeError> {
    raw.into_iter().map(ReceivedEnvelope::from_raw).collect()
}

/// `QueryResponse` with raw envelopes. Tags match `backend.proto`.
#[derive(Clone, PartialEq, Message)]
pub struct ReceivedQueryResponse {
    #[prost(bytes = "bytes", repeated, tag = "1")]
    pub envelopes: Vec<Bytes>,
    #[prost(message, optional, tag = "2")]
    pub continuation: Option<wire::Continuation>,
}

/// `Messages` from `SubscribeStaticResponse` or `SubscribeResponse`.
#[derive(Clone, PartialEq, Message)]
pub struct ReceivedMessages {
    #[prost(bytes = "bytes", repeated, tag = "1")]
    pub envelopes: Vec<Bytes>,
}

/// `SubscribeStaticResponse` with raw envelopes. Tags match `backend.proto`.
#[derive(Clone, PartialEq, Message)]
pub struct ReceivedStaticResponse {
    #[prost(oneof = "ReceivedStaticFrame", tags = "1, 2, 3")]
    pub response: Option<ReceivedStaticFrame>,
}

#[derive(Clone, PartialEq, prost::Oneof)]
pub enum ReceivedStaticFrame {
    #[prost(message, tag = "1")]
    Started(wire::subscribe_static_response::Started),
    #[prost(message, tag = "2")]
    Messages(ReceivedMessages),
    #[prost(message, tag = "3")]
    Keepalive(wire::subscribe_static_response::Keepalive),
}

/// `SubscribeResponse` with raw envelopes. Tags match `backend.proto`.
#[derive(Clone, PartialEq, Message)]
pub struct ReceivedSubscribeResponse {
    #[prost(oneof = "ReceivedSubscribeFrame", tags = "1, 2, 3, 4, 5")]
    pub response: Option<ReceivedSubscribeFrame>,
}

#[derive(Clone, PartialEq, prost::Oneof)]
pub enum ReceivedSubscribeFrame {
    #[prost(message, tag = "1")]
    Started(wire::subscribe_response::Started),
    #[prost(message, tag = "2")]
    Applied(wire::subscribe_response::Applied),
    #[prost(message, tag = "3")]
    Messages(ReceivedMessages),
    #[prost(message, tag = "4")]
    Ping(wire::Ping),
    #[prost(message, tag = "5")]
    Pong(wire::Pong),
}

xmtp_common::if_test! {
    /// A Welcome `ServerEnvelope` whose `WelcomeMessage` holds only `variant`
    /// under tag 3, which this schema does not define. Prost cannot build it.
    pub fn future_welcome_envelope(meta: &wire::EnvelopeMeta, variant: &[u8]) -> Bytes {
        use prost::encoding::{bytes, message};
        let (mut welcome, mut client, mut server) = (Vec::new(), Vec::new(), Vec::new());
        bytes::encode(3, &variant.to_vec(), &mut welcome);
        bytes::encode(2, &welcome, &mut client);
        message::encode(1, meta, &mut server);
        bytes::encode(2, &client, &mut server);
        server.into()
    }

    /// Equal when the received bytes are the canonical encoding of `other`.
    impl PartialEq<wire::ServerEnvelope> for ReceivedEnvelope {
        fn eq(&self, other: &wire::ServerEnvelope) -> bool {
            self.raw == other.encode_to_vec()
        }
    }

    impl From<wire::ServerEnvelope> for ReceivedEnvelope {
        fn from(envelope: wire::ServerEnvelope) -> Self {
            Self { meta: envelope.meta.clone(), raw: envelope.encode_to_vec().into() }
        }
    }

    impl From<wire::QueryResponse> for ReceivedQueryResponse {
        fn from(response: wire::QueryResponse) -> Self {
            Self {
                envelopes: response.envelopes.iter().map(|e| e.encode_to_vec().into()).collect(),
                continuation: response.continuation,
            }
        }
    }

    impl From<wire::SubscribeResponse> for ReceivedSubscribeResponse {
        fn from(response: wire::SubscribeResponse) -> Self {
            Self::decode(response.encode_to_vec().as_slice()).expect("same tags")
        }
    }
}
