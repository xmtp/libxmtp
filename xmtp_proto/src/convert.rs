use openmls::prelude::tls_codec::Deserialize;
use openmls::prelude::{MlsMessageIn, ProtocolMessage};

use crate::xmtp::identity::api::v1::PublishIdentityUpdateRequest;
use crate::xmtp::mls::api::v1::group_message_input::Version as GroupMessageInputVersion;
use crate::xmtp::mls::api::v1::{
    SendGroupMessagesRequest, SendWelcomeMessagesRequest, UploadKeyPackageRequest,
};
use crate::xmtp::xmtpv4::client_envelope::Payload;
use crate::xmtp::xmtpv4::{AuthenticatedData, ClientEnvelope};

mod inbox_id {

    use crate::xmtp::identity::MlsCredential;
    use openmls::{
        credentials::{errors::BasicCredentialError, BasicCredential},
        prelude::Credential as OpenMlsCredential,
    };
    use prost::Message;

    impl TryFrom<MlsCredential> for OpenMlsCredential {
        type Error = BasicCredentialError;

        fn try_from(proto: MlsCredential) -> Result<OpenMlsCredential, Self::Error> {
            let bytes = proto.encode_to_vec();
            Ok(BasicCredential::new(bytes).into())
        }
    }
}

impl From<SendWelcomeMessagesRequest> for ClientEnvelope {
    fn from(req: SendWelcomeMessagesRequest) -> Self {
        ClientEnvelope {
            aad: None,
            payload: Some(Payload::WelcomeMessage(
                req.messages.first().unwrap().clone(),
            )),
        }
    }
}

impl From<SendGroupMessagesRequest> for ClientEnvelope {
    fn from(req: SendGroupMessagesRequest) -> Self {
        let first_message = req.messages.first().unwrap();
        let version = match first_message.version.as_ref().unwrap() {
            GroupMessageInputVersion::V1(v1) => v1,
        };
        let topic = extract_topic_from_group_message(version.data.clone()).unwrap();
        ClientEnvelope {
            aad: Some(AuthenticatedData::with_topic(topic)),
            payload: Some(Payload::GroupMessage(req.messages.first().unwrap().clone())),
        }
    }
}

impl From<UploadKeyPackageRequest> for ClientEnvelope {
    fn from(req: UploadKeyPackageRequest) -> Self {
        ClientEnvelope {
            aad: Some(AuthenticatedData::dummy()),
            payload: Some(Payload::UploadKeyPackage(req)),
        }
    }
}

impl From<PublishIdentityUpdateRequest> for ClientEnvelope {
    fn from(req: PublishIdentityUpdateRequest) -> Self {
        ClientEnvelope {
            aad: Some(AuthenticatedData::dummy()),
            payload: Some(Payload::IdentityUpdate(req.identity_update.unwrap())),
        }
    }
}

impl AuthenticatedData {
    pub fn dummy() -> AuthenticatedData {
        AuthenticatedData {
            target_originator: 1,
            target_topic: vec![0x5],
            last_seen: None,
        }
    }

    pub fn with_topic(topic: String) -> AuthenticatedData {
        AuthenticatedData {
            target_originator: 1,
            target_topic: topic.as_bytes().to_vec(),
            last_seen: None,
        }
    }
}

fn extract_topic_from_group_message(message: Vec<u8>) -> Result<String, String> {
    let msg_result =
        MlsMessageIn::tls_deserialize(&mut message.as_slice()).map_err(|e| e.to_string())?;

    let protocol_message: ProtocolMessage = msg_result
        .try_into_protocol_message()
        .map_err(|e| e.to_string())?;

    Ok(format!(
        "g/{}",
        hex::encode(protocol_message.group_id().as_slice())
    ))
}
