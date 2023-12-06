use openmls::prelude::MlsMessageOut;
use prost::{DecodeError, Message};
use thiserror::Error;
use tls_codec::Serialize;
use xmtp_proto::xmtp::mls::database::{
    add_members_data::{Version as AddMembersVersion, V1 as AddMembersV1},
    post_commit_action::{Kind as PostCommitActionKind, SendWelcomes as SendWelcomesProto},
    remove_members_data::{Version as RemoveMembersVersion, V1 as RemoveMembersV1},
    send_message_data::{Version as SendMessageVersion, V1 as SendMessageV1},
    AddMembersData, PostCommitAction as PostCommitActionProto, RemoveMembersData, SendMessageData,
};

use crate::{types::Address, verified_key_package::KeyPackageVerificationError};

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

#[derive(Debug, Clone)]
pub struct SendMessageIntentData {
    pub message: Vec<u8>,
}

impl SendMessageIntentData {
    pub fn new(message: Vec<u8>) -> Self {
        Self { message }
    }

    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        SendMessageData {
            version: Some(SendMessageVersion::V1(SendMessageV1 {
                payload_bytes: self.message.clone(),
            })),
        }
        .encode(&mut buf)
        .unwrap();

        buf
    }

    pub(crate) fn from_bytes(data: &[u8]) -> Result<Self, IntentError> {
        let msg = SendMessageData::decode(data)?;
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

#[derive(Debug, Clone)]
pub struct AddMembersIntentData {
    pub account_addresses: Vec<Address>,
}

impl AddMembersIntentData {
    pub fn new(account_addresses: Vec<Address>) -> Self {
        Self { account_addresses }
    }

    pub(crate) fn to_bytes(self) -> Result<Vec<u8>, IntentError> {
        let mut buf = Vec::new();
        AddMembersData {
            version: Some(AddMembersVersion::V1(AddMembersV1 {
                account_addresses: self.account_addresses,
            })),
        }
        .encode(&mut buf)
        .expect("encode error");

        Ok(buf)
    }

    pub(crate) fn from_bytes(data: &[u8]) -> Result<Self, IntentError> {
        let msg = AddMembersData::decode(data)?;
        let addresses = match msg.version {
            Some(AddMembersVersion::V1(v1)) => v1.account_addresses,
            None => return Err(IntentError::Generic("missing payload".to_string())),
        };

        Ok(Self::new(addresses))
    }
}

impl TryFrom<AddMembersIntentData> for Vec<u8> {
    type Error = IntentError;

    fn try_from(intent: AddMembersIntentData) -> Result<Self, Self::Error> {
        intent.to_bytes()
    }
}

#[derive(Debug, Clone)]
pub struct RemoveMembersIntentData {
    pub account_addresses: Vec<String>,
}

impl RemoveMembersIntentData {
    pub fn new(account_addresses: Vec<String>) -> Self {
        Self { account_addresses }
    }

    pub(crate) fn to_bytes(self) -> Vec<u8> {
        let mut buf = Vec::new();

        RemoveMembersData {
            version: Some(RemoveMembersVersion::V1(RemoveMembersV1 {
                account_addresses: self.account_addresses,
            })),
        }
        .encode(&mut buf)
        .expect("encode error");

        buf
    }

    pub(crate) fn from_bytes(data: &[u8]) -> Result<Self, IntentError> {
        let msg = RemoveMembersData::decode(data)?;
        let account_addresses = match msg.version {
            Some(RemoveMembersVersion::V1(v1)) => v1.account_addresses,
            None => return Err(IntentError::Generic("missing payload".to_string())),
        };

        Ok(Self::new(account_addresses))
    }
}

impl From<RemoveMembersIntentData> for Vec<u8> {
    fn from(intent: RemoveMembersIntentData) -> Self {
        intent.to_bytes()
    }
}

#[derive(Debug, Clone)]
pub enum PostCommitAction {
    SendWelcomes(SendWelcomesAction),
}

#[derive(Debug, Clone)]
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

#[cfg(test)]
mod tests {
    use xmtp_cryptography::utils::generate_local_wallet;

    use super::*;
    use crate::{builder::ClientBuilder, InboxOwner};

    #[test]
    fn test_serialize_send_message() {
        let message = vec![1, 2, 3];
        let intent = SendMessageIntentData::new(message.clone());
        let as_bytes: Vec<u8> = intent.into();
        let restored_intent = SendMessageIntentData::from_bytes(as_bytes.as_slice()).unwrap();

        assert_eq!(restored_intent.message, message);
    }

    #[tokio::test]
    async fn test_serialize_add_members() {
        let wallet = generate_local_wallet();
        let wallet_address = wallet.get_address();

        let intent = AddMembersIntentData::new(vec![wallet_address.clone()]);
        let as_bytes: Vec<u8> = intent.clone().try_into().unwrap();
        let restored_intent = AddMembersIntentData::from_bytes(as_bytes.as_slice()).unwrap();

        assert!(intent.account_addresses[0].eq(&restored_intent.account_addresses[0]));
    }
}
