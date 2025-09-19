//! generate implementations for fake messages that include fake data but compatible with openmls types
use super::Generate;
use xmtp_cryptography::{
    openmls::{
        prelude::{
            MlsMessageIn, MlsMessageOut, ProtocolMessage, PublicMessage, tls_codec::Serialize,
        },
        test_utils::frankenstein::{
            FrankenCommit, FrankenContentType, FrankenFramedContent, FrankenFramedContentAuthData,
            FrankenFramedContentBody, FrankenMlsMessage, FrankenMlsMessageBody,
            FrankenPrivateMessage, FrankenPublicMessage, FrankenSender,
        },
    },
    rand::rand_vec,
};

pub struct FakeMlsApplicationMessage {
    inner: FrankenMlsMessage,
}

impl Generate for FakeMlsApplicationMessage {
    fn generate() -> Self {
        let mut msg = FrankenPublicMessage::generate();
        msg.content.body = FrankenFramedContentBody::Application(rand_vec::<16>().into());
        Self {
            inner: FrankenMlsMessage {
                version: 1,
                body: FrankenMlsMessageBody::PublicMessage(msg),
            },
        }
    }
}

impl From<FakeMlsApplicationMessage> for MlsMessageOut {
    fn from(value: FakeMlsApplicationMessage) -> Self {
        value.inner.into()
    }
}

/// a fake mls commit message populated with garbage data
/// can be transformed into MlsMessageOut/ProtocolMessage
pub struct FakeMlsCommitMessage {
    inner: FrankenMlsMessage,
}

impl Generate for FakeMlsCommitMessage {
    fn generate() -> Self {
        let mut msg = FrankenPrivateMessage::generate();
        msg.content_type = FrankenContentType::Commit;
        Self {
            inner: FrankenMlsMessage {
                version: 1,
                body: FrankenMlsMessageBody::PrivateMessage(msg),
            },
        }
    }
}

impl From<FakeMlsCommitMessage> for MlsMessageOut {
    fn from(value: FakeMlsCommitMessage) -> Self {
        value.inner.into()
    }
}

impl From<FakeMlsCommitMessage> for ProtocolMessage {
    fn from(value: FakeMlsCommitMessage) -> Self {
        ProtocolMessage::try_from(MlsMessageIn::from(MlsMessageOut::from(value))).unwrap()
    }
}

impl Generate for FrankenPrivateMessage {
    fn generate() -> Self {
        FrankenPrivateMessage {
            group_id: rand_vec::<16>().into(),
            epoch: rand_vec::<4>().into(),
            content_type: FrankenContentType::Application,
            authenticated_data: rand_vec::<16>().into(),
            encrypted_sender_data: rand_vec::<16>().into(),
            ciphertext: rand_vec::<16>().into(),
        }
    }
}

impl Generate for FrankenCommit {
    fn generate() -> Self {
        todo!()
    }
}

impl Generate for PublicMessage {
    fn generate() -> Self {
        FrankenPublicMessage::generate().into()
    }
}

impl Generate for FrankenPublicMessage {
    fn generate() -> Self {
        FrankenPublicMessage {
            content: FrankenFramedContent::generate(),
            auth: FrankenFramedContentAuthData::generate(),
            membership_tag: None,
        }
    }
}

impl Generate for FrankenFramedContent {
    fn generate() -> Self {
        FrankenFramedContent {
            group_id: rand_vec::<16>().into(),
            epoch: crate::rand_u64(),
            sender: FrankenSender::Member(0),
            authenticated_data: FrankenFramedContentAuthData::generate()
                .tls_serialize_detached()
                .unwrap()
                .into(),
            body: FrankenFramedContentBody::Application(rand_vec::<16>().into()),
        }
    }
}

impl Generate for FrankenFramedContentAuthData {
    fn generate() -> Self {
        FrankenFramedContentAuthData {
            signature: vec![].into(),
            // not specifying this (leaving as `None`) causes conversion/serialization to fail.
            confirmation_tag: Some(rand_vec::<8>().into()),
        }
    }
}
