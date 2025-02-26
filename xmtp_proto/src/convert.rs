use prost::Message;
use crate::xmtp::identity::api::v1::{PublishIdentityUpdateRequest};
use crate::xmtp::mls::api::v1::{
    group_message_input::Version as GroupMessageInputVersion,
    welcome_message_input::Version as WelcomeMessageVersion, GroupMessageInput,
    UploadKeyPackageRequest, WelcomeMessageInput,
};
use crate::xmtp::xmtpv4::envelopes::client_envelope::Payload;
use crate::xmtp::xmtpv4::envelopes::{AuthenticatedData, ClientEnvelope, OriginatorEnvelope, UnsignedOriginatorEnvelope};

use crate::v4_utils::{
    build_identity_topic_from_hex_encoded, build_welcome_message_topic, extract_client_envelope,
    get_group_message_topic, get_key_package_topic,
};
use crate::xmtp::identity::api::v1::get_identity_updates_response::IdentityUpdateLog;
use crate::xmtp::mls::api::v1::fetch_key_packages_response::KeyPackage;

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

impl TryFrom<UploadKeyPackageRequest> for ClientEnvelope {
    type Error = crate::ProtoError;

    fn try_from(req: UploadKeyPackageRequest) -> Result<Self, Self::Error> {
        if let Some(key_package) = req.key_package.as_ref() {
            Ok(ClientEnvelope {
                aad: Some(AuthenticatedData::with_topic(get_key_package_topic(
                    key_package,
                )?)),
                payload: Some(Payload::UploadKeyPackage(req)),
            })
        } else {
            Err(crate::ProtoError::NotFound("payload keypackage".into()))
        }
    }
}

impl TryFrom<OriginatorEnvelope> for KeyPackage {
    type Error = crate::ProtoError;

    fn try_from(originator: OriginatorEnvelope) -> Result<Self, Self::Error> {
        let client_env = extract_client_envelope(&originator)?;

        if let Some(Payload::UploadKeyPackage(upload_key_package)) = client_env.payload {
            let key_package = upload_key_package
                .key_package
                .ok_or_else(|| crate::ProtoError::NotFound("payload key package".into()))?;

            Ok(KeyPackage {
                key_package_tls_serialized: key_package.key_package_tls_serialized,
            })
        } else {
            Err(crate::ProtoError::NotFound("payload key package".into()))
        }
    }
}

impl TryFrom<OriginatorEnvelope> for IdentityUpdateLog {
    type Error = crate::ProtoError;

    fn try_from(envelope: OriginatorEnvelope) -> Result<Self, Self::Error> {
        let mut unsigned_originator_envelope = envelope.unsigned_originator_envelope.as_slice();
        let originator_envelope = UnsignedOriginatorEnvelope::decode(&mut unsigned_originator_envelope)
            .map_err(|_| crate::ProtoError::NotFound("Failed to decode UnsignedOriginatorEnvelope".into()))?;

        let payer_envelope = originator_envelope
            .payer_envelope
            .ok_or_else(|| crate::ProtoError::NotFound("payer envelope".into()))?;

        // TODO: validate payer signatures
        let mut unsigned_client_envelope = payer_envelope.unsigned_client_envelope.as_slice();
        let client_envelope = ClientEnvelope::decode(&mut unsigned_client_envelope)
            .map_err(|_| crate::ProtoError::NotFound("Failed to decode ClientEnvelope".into()))?;

        let payload = client_envelope
            .payload
            .ok_or_else(|| crate::ProtoError::NotFound("payload".into()))?;

        let identity_update = match payload {
            Payload::IdentityUpdate(update) => update,
            _ => return Err(crate::ProtoError::NotFound("Expected IdentityUpdate payload".into())),
        };

        Ok(IdentityUpdateLog {
            sequence_id: originator_envelope.originator_sequence_id,
            server_timestamp_ns: originator_envelope.originator_ns as u64,
            update: Some(identity_update),
        })
    }
}
impl TryFrom<PublishIdentityUpdateRequest> for ClientEnvelope {
    type Error = crate::ProtoError;

    fn try_from(req: PublishIdentityUpdateRequest) -> Result<Self, Self::Error> {
        if let Some(identity_update) = req.identity_update {
            Ok(ClientEnvelope {
                aad: Some(AuthenticatedData::with_topic(
                    build_identity_topic_from_hex_encoded(&identity_update.inbox_id)?,
                )),
                payload: Some(Payload::IdentityUpdate(identity_update)),
            })
        } else {
            Err(crate::ProtoError::NotFound(
                "payload identity update".into(),
            ))
        }
    }
}

impl TryFrom<GroupMessageInput> for ClientEnvelope {
    type Error = crate::ProtoError;

    fn try_from(req: GroupMessageInput) -> Result<Self, Self::Error> {
        if let Some(GroupMessageInputVersion::V1(ref version)) = req.version {
            Ok(ClientEnvelope {
                aad: Some(AuthenticatedData::with_topic(get_group_message_topic(
                    version.data.clone(),
                )?)),
                payload: Some(Payload::GroupMessage(req)),
            })
        } else {
            Err(crate::ProtoError::NotFound("payload group message".into()))
        }
    }
}

impl TryFrom<WelcomeMessageInput> for ClientEnvelope {
    type Error = crate::ProtoError;

    fn try_from(req: WelcomeMessageInput) -> Result<Self, Self::Error> {
        if let Some(WelcomeMessageVersion::V1(ref version)) = req.version {
            Ok(ClientEnvelope {
                aad: Some(AuthenticatedData::with_topic(build_welcome_message_topic(
                    version.installation_key.as_slice(),
                ))),
                payload: Some(Payload::WelcomeMessage(req)),
            })
        } else {
            Err(crate::ProtoError::NotFound(
                "payload welcome message".into(),
            ))
        }
    }
}

impl AuthenticatedData {
    #[allow(deprecated)]
    pub fn with_topic(topic: Vec<u8>) -> AuthenticatedData {
        AuthenticatedData {
            target_originator: None,
            target_topic: topic,
            depends_on: None,
            is_commit: false,
        }
    }
}
