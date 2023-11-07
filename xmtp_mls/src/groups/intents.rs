use openmls::{error, prelude::MlsMessageOut};
use prost::{DecodeError, Message};
use thiserror::Error;
use tls_codec::Serialize;
use xmtp_proto::xmtp::mls::database::{
    add_members_publish_data::{Version as AddMembersVersion, V1 as AddMembersV1},
    post_commit_action::{Kind as PostCommitActionKind, SendWelcomes as SendWelcomesProto},
    send_message_publish_data::{Version as SendMessageVersion, V1 as SendMessageV1},
    AddMembersPublishData, PostCommitAction as PostCommitActionProto, SendMessagePublishData,
};

use crate::{
    verified_key_package::{KeyPackageVerificationError, VerifiedKeyPackage},
    xmtp_openmls_provider::XmtpOpenMlsProvider,
};

#[derive(Debug, Error)]
pub enum IntentError {
    #[error("decode error: {0}")]
    Decode(#[from] DecodeError),
    #[error("key package verification: {0}")]
    KeyPackageVerification(#[from] KeyPackageVerificationError),
    #[error("tls codec: {0}")]
    TlsCodec(#[from] tls_codec::Error),
    #[error("generic: {0}")]
    Generic(String),
}

pub struct SendMessageIntentData {
    pub message: Vec<u8>,
}

impl SendMessageIntentData {
    pub fn new(message: Vec<u8>) -> Self {
        Self { message: message }
    }

    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        SendMessagePublishData {
            version: Some(SendMessageVersion::V1(SendMessageV1 {
                payload_bytes: self.message.clone(),
            })),
        }
        .encode(&mut buf)
        .unwrap();

        buf
    }

    pub(crate) fn from_bytes(data: &[u8]) -> Result<Self, IntentError> {
        let msg = SendMessagePublishData::decode(data)?;
        let payload_bytes = match msg.version {
            Some(SendMessageVersion::V1(v1)) => v1.payload_bytes,
            None => return Err(IntentError::Generic("missing payload".to_string())),
        };

        Ok(Self::new(payload_bytes))
    }
}

impl From<SendMessageIntentData> for Vec<u8> {
    fn from(intent: SendMessageIntentData) -> Self {
        intent.to_bytes()
    }
}

pub struct AddMembersIntentData {
    pub key_packages: Vec<VerifiedKeyPackage>,
}

impl AddMembersIntentData {
    pub fn new(key_packages: Vec<VerifiedKeyPackage>) -> Self {
        Self { key_packages }
    }

    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        let key_package_bytes: Vec<Vec<u8>> = self
            .key_packages
            .iter()
            .map(|kp| kp.inner.tls_serialize_detached().unwrap())
            .collect();

        AddMembersPublishData {
            version: Some(AddMembersVersion::V1(AddMembersV1 {
                key_packages_bytes_tls_serialized: key_package_bytes,
            })),
        }
        .encode(&mut buf)
        .unwrap();

        buf
    }

    pub(crate) fn from_bytes(
        data: &[u8],
        provider: &XmtpOpenMlsProvider,
    ) -> Result<Self, IntentError> {
        let msg = AddMembersPublishData::decode(data)?;
        let key_package_bytes = match msg.version {
            Some(AddMembersVersion::V1(v1)) => v1.key_packages_bytes_tls_serialized,
            None => return Err(IntentError::Generic("missing payload".to_string())),
        };
        let key_packages: Result<Vec<VerifiedKeyPackage>, KeyPackageVerificationError> =
            key_package_bytes
                .iter()
                .map(|kp| VerifiedKeyPackage::from_bytes(provider, kp))
                .collect();

        Ok(Self::new(key_packages?))
    }
}

impl From<AddMembersIntentData> for Vec<u8> {
    fn from(intent: AddMembersIntentData) -> Self {
        intent.to_bytes()
    }
}

pub enum PostCommitAction {
    SendWelcomes(SendWelcomesAction),
}

pub struct SendWelcomesAction {
    pub installation_ids: Vec<Vec<u8>>,
    pub welcome_message: Vec<u8>,
}

impl SendWelcomesAction {
    pub fn new(installation_ids: Vec<Vec<u8>>, welcome_message: Vec<u8>) -> Self {
        Self {
            installation_ids,
            welcome_message,
        }
    }

    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        PostCommitActionProto {
            kind: Some(PostCommitActionKind::SendWelcomes(SendWelcomesProto {
                installation_ids: self.installation_ids.clone(),
                welcome_message: self.welcome_message.clone(),
            })),
        }
        .encode(&mut buf)
        .unwrap();

        buf
    }
}

impl PostCommitAction {
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        match self {
            PostCommitAction::SendWelcomes(action) => action.to_bytes(),
        }
    }

    pub(crate) fn from_bytes(data: &[u8]) -> Result<Self, IntentError> {
        let decoded = PostCommitActionProto::decode(data)?;
        match decoded.kind {
            Some(PostCommitActionKind::SendWelcomes(proto)) => Ok(Self::SendWelcomes(
                SendWelcomesAction::new(proto.installation_ids, proto.welcome_message),
            )),
            None => Err(IntentError::Generic(
                "missing post commit action".to_string(),
            )),
        }
    }

    pub(crate) fn from_welcome(
        welcome: MlsMessageOut,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<Self, IntentError> {
        let welcome_bytes = welcome.tls_serialize_detached()?;

        Ok(Self::SendWelcomes(SendWelcomesAction::new(
            installation_ids,
            welcome_bytes,
        )))
    }
}

impl From<Vec<u8>> for PostCommitAction {
    fn from(data: Vec<u8>) -> Self {
        PostCommitAction::from_bytes(data.as_slice()).unwrap()
    }
}
