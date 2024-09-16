use crate::xmtp::identity::api::v1::PublishIdentityUpdateRequest;
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
        ClientEnvelope {
            aad: Some(AuthenticatedData::dummy()),
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
}
