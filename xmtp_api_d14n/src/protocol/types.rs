use xmtp_proto::xmtp::identity::associations::IdentityUpdate;
use xmtp_proto::xmtp::mls::api::v1::{
    GroupMessageInput, UploadKeyPackageRequest, WelcomeMessageInput,
};
use xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope;
use xmtp_proto::xmtp::xmtpv4::envelopes::client_envelope::Payload;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TopicKind {
    GroupMessagesV1 = 0,
    WelcomeMessagesV1 = 1,
    IdentityUpdatesV1 = 2,
    KeyPackagesV1 = 3,
}

impl TopicKind {
    pub fn build(&self, bytes: &[u8]) -> Vec<u8> {
        let mut topic = Vec::with_capacity(1 + bytes.len());
        topic.push(*self as u8);
        topic.extend_from_slice(bytes);
        topic
    }
}

/// A topic where the first byte is the kind
/// https://github.com/xmtp/XIPs/blob/main/XIPs/xip-49-decentralized-backend.md#332-envelopes
pub struct Topic {
    kind: TopicKind,
    bytes: Vec<u8>,
}

impl Topic {
    fn bytes(&self) -> Vec<u8> {
        self.kind.build(&self.bytes)
    }
}

impl From<Topic> for Vec<u8> {
    fn from(topic: Topic) -> Vec<u8> {
        topic.bytes().to_vec()
    }
}

#[derive(Debug, Clone)]
pub enum PayloadRef<'a> {
    GroupMessage(&'a GroupMessageInput),
    WelcomeMessage(&'a WelcomeMessageInput),
    UploadKeyPackage(&'a UploadKeyPackageRequest),
    IdentityUpdate(&'a IdentityUpdate),
}

impl<'a> From<PayloadRef<'a>> for Payload {
    fn from(payload: PayloadRef<'a>) -> Payload {
        use PayloadRef::*;
        match payload {
            GroupMessage(m) => Payload::GroupMessage(m.clone()),
            WelcomeMessage(m) => Payload::WelcomeMessage(m.clone()),
            UploadKeyPackage(m) => Payload::UploadKeyPackage(m.clone()),
            IdentityUpdate(m) => Payload::IdentityUpdate(m.clone()),
        }
    }
}

/// References a ClientEnvelope
pub struct ClientEnvelopeRef<'a> {
    inner: &'a ClientEnvelope,
}

impl<'a> From<ClientEnvelopeRef<'a>> for ClientEnvelope {
    fn from(envelope: ClientEnvelopeRef<'a>) -> ClientEnvelope {
        envelope.inner.clone()
    }
}
