use crate::v4_utils::{
    build_identity_topic_from_hex_encoded, build_welcome_message_topic, extract_client_envelope,
    get_group_message_topic, get_key_package_topic,
};
use crate::xmtp::identity::api::v1::get_identity_updates_response::IdentityUpdateLog;
use crate::xmtp::identity::api::v1::PublishIdentityUpdateRequest;
use crate::xmtp::mls::api::v1::fetch_key_packages_response::KeyPackage;
use crate::xmtp::mls::api::v1::{
    group_message_input::Version as GroupMessageInputVersion,
    welcome_message_input::Version as WelcomeMessageVersion, GroupMessageInput,
    UploadKeyPackageRequest, WelcomeMessageInput,
};
use crate::xmtp::xmtpv4::envelopes::client_envelope::Payload;
use crate::xmtp::xmtpv4::envelopes::{AuthenticatedData, ClientEnvelope, OriginatorEnvelope};
use crate::ConversionError;

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
    type Error = ConversionError;

    fn try_from(req: UploadKeyPackageRequest) -> Result<Self, Self::Error> {
        if let Some(key_package) = req.key_package.as_ref() {
            let topic =
                get_key_package_topic(key_package).map_err(|_| ConversionError::Missing {
                    item: "topic",
                    r#type: std::any::type_name::<UploadKeyPackageRequest>(),
                })?;

            Ok(ClientEnvelope {
                aad: Some(AuthenticatedData::with_topic(topic)),
                payload: Some(Payload::UploadKeyPackage(req)),
            })
        } else {
            Err(ConversionError::Missing {
                item: "key_package",
                r#type: std::any::type_name::<UploadKeyPackageRequest>(),
            })
        }
    }
}

impl TryFrom<OriginatorEnvelope> for KeyPackage {
    type Error = ConversionError;

    fn try_from(originator: OriginatorEnvelope) -> Result<Self, Self::Error> {
        let client_env =
            extract_client_envelope(&originator).map_err(|_| ConversionError::Missing {
                item: "client_env",
                r#type: std::any::type_name::<OriginatorEnvelope>(),
            })?;

        if let Some(Payload::UploadKeyPackage(upload_key_package)) = client_env.payload {
            let key_package = upload_key_package
                .key_package
                .ok_or(ConversionError::Missing {
                    item: "key_package",
                    r#type: std::any::type_name::<OriginatorEnvelope>(),
                })?;

            Ok(KeyPackage {
                key_package_tls_serialized: key_package.key_package_tls_serialized,
            })
        } else {
            Err(ConversionError::Missing {
                item: "key_package",
                r#type: std::any::type_name::<OriginatorEnvelope>(),
            })
        }
    }
}

impl TryFrom<OriginatorEnvelope> for IdentityUpdateLog {
    type Error = ConversionError;

    fn try_from(_envelope: OriginatorEnvelope) -> Result<Self, Self::Error> {
        // temporary block until this function is updated to handle payer_envelope_bytes
        Err(ConversionError::Missing {
            item: "identity_update",
            r#type: std::any::type_name::<OriginatorEnvelope>(),
        })

        //let mut unsigned_originator_envelope = envelope.unsigned_originator_envelope.as_slice();
        //let originator_envelope = UnsignedOriginatorEnvelope::decode(
        //    &mut unsigned_originator_envelope,
        //)
        //.map_err(|_| ConversionError::Missing {
        //    item: "identity_update",
        //    r#type: std::any::type_name::<OriginatorEnvelope>(),
        //})?;

        // let payer_envelope =
        // originator_envelope
        // .payer_envelope
        // .ok_or(ConversionError::Missing {
        // item: "identity_update",
        // r#type: std::any::type_name::<OriginatorEnvelope>(),
        // })?;

        // TODO: validate payer signatures
        // let mut unsigned_client_envelope = payer_envelope.unsigned_client_envelope.as_slice();
        // let client_envelope =
        // ClientEnvelope::decode(&mut unsigned_client_envelope).map_err(|_| {
        // ConversionError::Missing {
        // item: "identity_update",
        // r#type: std::any::type_name::<OriginatorEnvelope>(),
        // }
        // })?;

        // let payload = client_envelope.payload.ok_or(ConversionError::Missing {
        // item: "identity_update",
        // r#type: std::any::type_name::<OriginatorEnvelope>(),
        // })?;

        // let identity_update = match payload {
        // Payload::IdentityUpdate(update) => update,
        // _ => {
        // return Err(ConversionError::Missing {
        // item: "identity_update",
        // r#type: std::any::type_name::<OriginatorEnvelope>(),
        // })
        // }
        // };

        // Ok(IdentityUpdateLog {
        // sequence_id: originator_envelope.originator_sequence_id,
        // server_timestamp_ns: originator_envelope.originator_ns as u64,
        // update: Some(identity_update),
        // })
    }
}
impl TryFrom<PublishIdentityUpdateRequest> for ClientEnvelope {
    type Error = ConversionError;

    fn try_from(req: PublishIdentityUpdateRequest) -> Result<Self, Self::Error> {
        if let Some(identity_update) = req.identity_update {
            Ok(ClientEnvelope {
                aad: Some(AuthenticatedData::with_topic(
                    build_identity_topic_from_hex_encoded(&identity_update.inbox_id).map_err(
                        |_| ConversionError::Missing {
                            item: "identity_update",
                            r#type: std::any::type_name::<PublishIdentityUpdateRequest>(),
                        },
                    )?,
                )),
                payload: Some(Payload::IdentityUpdate(identity_update)),
            })
        } else {
            Err(ConversionError::Missing {
                item: "identity_update",
                r#type: std::any::type_name::<PublishIdentityUpdateRequest>(),
            })
        }
    }
}

impl TryFrom<GroupMessageInput> for ClientEnvelope {
    type Error = ConversionError;

    fn try_from(req: GroupMessageInput) -> Result<Self, Self::Error> {
        if let Some(GroupMessageInputVersion::V1(ref version)) = req.version {
            Ok(ClientEnvelope {
                aad: Some(AuthenticatedData::with_topic(
                    get_group_message_topic(version.data.clone()).map_err(|_| {
                        ConversionError::Missing {
                            item: "group_message",
                            r#type: std::any::type_name::<GroupMessageInput>(),
                        }
                    })?,
                )),
                payload: Some(Payload::GroupMessage(req)),
            })
        } else {
            Err(ConversionError::Missing {
                item: "group_message",
                r#type: std::any::type_name::<GroupMessageInput>(),
            })
        }
    }
}

impl TryFrom<WelcomeMessageInput> for ClientEnvelope {
    type Error = ConversionError;

    fn try_from(req: WelcomeMessageInput) -> Result<Self, Self::Error> {
        if let Some(WelcomeMessageVersion::V1(ref version)) = req.version {
            Ok(ClientEnvelope {
                aad: Some(AuthenticatedData::with_topic(build_welcome_message_topic(
                    version.installation_key.as_slice(),
                ))),
                payload: Some(Payload::WelcomeMessage(req)),
            })
        } else {
            Err(ConversionError::Missing {
                item: "welcome_message",
                r#type: std::any::type_name::<WelcomeMessageInput>(),
            })
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
