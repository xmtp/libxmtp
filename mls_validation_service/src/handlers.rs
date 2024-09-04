use futures::future::join_all;
use openmls::prelude::{tls_codec::Deserialize, MlsMessageIn, ProtocolMessage};
use openmls_rust_crypto::RustCrypto;
use tonic::{Request, Response, Status};

use xmtp_id::associations::{self, try_map_vec, AssociationError, DeserializationError};
use xmtp_mls::{
    utils::id::serialize_group_id,
    verified_key_package_v2::{KeyPackageVerificationError, VerifiedKeyPackageV2},
};
use xmtp_proto::xmtp::{
    identity::associations::IdentityUpdate as IdentityUpdateProto,
    mls_validation::v1::{
        validate_group_messages_response::ValidationResponse as ValidateGroupMessageValidationResponse,
        validate_inbox_id_key_packages_response::Response as ValidateInboxIdKeyPackageResponse,
        validation_api_server::ValidationApi, GetAssociationStateRequest,
        GetAssociationStateResponse, ValidateGroupMessagesRequest, ValidateGroupMessagesResponse,
        ValidateInboxIdKeyPackagesRequest, ValidateInboxIdKeyPackagesResponse,
    },
};

#[derive(Debug, thiserror::Error)]
pub enum GrpcServerError {
    #[error(transparent)]
    Deserialization(#[from] DeserializationError),
    #[error(transparent)]
    Association(#[from] AssociationError),
}

impl From<GrpcServerError> for Status {
    fn from(err: GrpcServerError) -> Self {
        Status::invalid_argument(err.to_string())
    }
}

#[derive(Debug, Default)]
pub struct ValidationService {}

#[tonic::async_trait]
impl ValidationApi for ValidationService {
    async fn validate_group_messages(
        &self,
        request: Request<ValidateGroupMessagesRequest>,
    ) -> Result<Response<ValidateGroupMessagesResponse>, Status> {
        let out: Vec<ValidateGroupMessageValidationResponse> = request
            .into_inner()
            .group_messages
            .into_iter()
            .map(|message| {
                match validate_group_message(message.group_message_bytes_tls_serialized) {
                    Ok(res) => ValidateGroupMessageValidationResponse {
                        group_id: res.group_id,
                        error_message: "".to_string(),
                        is_ok: true,
                    },
                    Err(e) => ValidateGroupMessageValidationResponse {
                        group_id: "".to_string(),
                        error_message: e,
                        is_ok: false,
                    },
                }
            })
            .collect();

        Ok(Response::new(ValidateGroupMessagesResponse {
            responses: out,
        }))
    }

    async fn get_association_state(
        &self,
        request: Request<GetAssociationStateRequest>,
    ) -> Result<Response<GetAssociationStateResponse>, Status> {
        let GetAssociationStateRequest {
            old_updates,
            new_updates,
        } = request.into_inner();

        get_association_state(old_updates, new_updates)
            .await
            .map(Response::new)
            .map_err(Into::into)
    }

    async fn validate_inbox_id_key_packages(
        &self,
        request: Request<ValidateInboxIdKeyPackagesRequest>,
    ) -> Result<Response<ValidateInboxIdKeyPackagesResponse>, Status> {
        let ValidateInboxIdKeyPackagesRequest { key_packages } = request.into_inner();

        let responses: Vec<_> = key_packages
            .into_iter()
            .map(|k| k.key_package_bytes_tls_serialized)
            .map(validate_inbox_id_key_package)
            .collect();

        let responses: Vec<ValidateInboxIdKeyPackageResponse> = join_all(responses)
            .await
            .into_iter()
            .map(|res| res.map_err(ValidateInboxIdKeyPackageResponse::from))
            .map(|r| r.unwrap_or_else(|e| e))
            .collect();

        Ok(Response::new(ValidateInboxIdKeyPackagesResponse {
            responses,
        }))
    }
}

#[derive(thiserror::Error, Debug)]
enum ValidateInboxIdKeyPackageError {
    #[error("XMTP Key Package failed {0}")]
    KeyPackageVerification(#[from] KeyPackageVerificationError),
}

impl From<ValidateInboxIdKeyPackageError> for ValidateInboxIdKeyPackageResponse {
    fn from(error: ValidateInboxIdKeyPackageError) -> ValidateInboxIdKeyPackageResponse {
        ValidateInboxIdKeyPackageResponse {
            is_ok: false,
            error_message: error.to_string(),
            credential: None,
            installation_public_key: vec![],
            expiration: 0,
        }
    }
}

async fn validate_inbox_id_key_package(
    key_package: Vec<u8>,
) -> Result<ValidateInboxIdKeyPackageResponse, ValidateInboxIdKeyPackageError> {
    let rust_crypto = RustCrypto::default();
    let kp = VerifiedKeyPackageV2::from_bytes(&rust_crypto, key_package.as_slice())?;

    Ok(ValidateInboxIdKeyPackageResponse {
        is_ok: true,
        error_message: "".into(),
        credential: Some(kp.credential),
        installation_public_key: kp.installation_public_key,
        // We are deprecating the expiration field and key package lifetimes, so stop checking for its existence
        expiration: 0,
    })
}

async fn get_association_state(
    old_updates: Vec<IdentityUpdateProto>,
    new_updates: Vec<IdentityUpdateProto>,
) -> Result<GetAssociationStateResponse, GrpcServerError> {
    let (old_updates, new_updates) = (try_map_vec(old_updates)?, try_map_vec(new_updates)?);

    if old_updates.is_empty() {
        let new_state = associations::get_state(&new_updates).await?;
        return Ok(GetAssociationStateResponse {
            association_state: Some(new_state.clone().into()),
            state_diff: Some(new_state.as_diff().into()),
        });
    }

    let old_state = associations::get_state(&old_updates).await?;
    let mut new_state = old_state.clone();
    for update in new_updates {
        new_state = associations::apply_update(new_state, update).await?;
    }

    let state_diff = old_state.diff(&new_state);

    Ok(GetAssociationStateResponse {
        association_state: Some(new_state.into()),
        state_diff: Some(state_diff.into()),
    })
}

struct ValidateGroupMessageResult {
    group_id: String,
}

fn validate_group_message(message: Vec<u8>) -> Result<ValidateGroupMessageResult, String> {
    let msg_result =
        MlsMessageIn::tls_deserialize(&mut message.as_slice()).map_err(|e| e.to_string())?;

    let protocol_message: ProtocolMessage = msg_result
        .try_into_protocol_message()
        .map_err(|e| e.to_string())?;

    Ok(ValidateGroupMessageResult {
        group_id: serialize_group_id(protocol_message.group_id().as_slice()),
    })
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;
    use ethers::signers::{LocalWallet, Signer as _};
    use openmls::{
        extensions::{ApplicationIdExtension, Extension, Extensions},
        prelude::{
            tls_codec::Serialize, Ciphersuite, Credential as OpenMlsCredential, CredentialWithKey,
        },
        prelude_test::KeyPackage,
    };
    use openmls_basic_credential::SignatureKeyPair;
    use openmls_rust_crypto::OpenMlsRustCrypto;
    use sha2::{Digest, Sha512};
    use xmtp_id::{
        associations::{
            generate_inbox_id,
            unsigned_actions::{
                SignatureTextCreator as _, UnsignedAction, UnsignedAddAssociation,
                UnsignedCreateInbox, UnsignedIdentityUpdate,
            },
            Action, AddAssociation, CreateInbox, IdentityUpdate, InstallationKeySignature,
            MemberIdentifier, RecoverableEcdsaSignature,
        },
        constants::INSTALLATION_KEY_SIGNATURE_CONTEXT,
    };
    use xmtp_proto::xmtp::identity::{
        associations::IdentityUpdate as IdentityUpdateProto, MlsCredential as InboxIdMlsCredential,
    };
    use xmtp_proto::xmtp::mls_validation::v1::validate_inbox_id_key_packages_request::KeyPackage as KeyPackageProtoWrapper;

    use super::*;

    const CIPHERSUITE: Ciphersuite =
        Ciphersuite::MLS_128_DHKEMX25519_CHACHA20POLY1305_SHA256_Ed25519;

    async fn generate_inbox_id_credential() -> (String, LocalWallet, SigningKey, CreateInbox) {
        let signing_key = SigningKey::generate(&mut rand::thread_rng());

        let wallet = LocalWallet::new(&mut rand::thread_rng());
        let address = format!("0x{}", hex::encode(wallet.address()));

        let inbox_id = generate_inbox_id(&address, &0);

        let unsigned_action = UnsignedAction::CreateInbox(UnsignedCreateInbox {
            nonce: 0,
            account_address: address.clone(),
        });

        let update = UnsignedIdentityUpdate {
            client_timestamp_ns: 1_000_000u64,
            inbox_id: inbox_id.clone(),
            actions: vec![unsigned_action],
        };

        let signature = wallet
            .sign_message(update.signature_text())
            .await
            .unwrap()
            .to_vec();

        let ecdsa_signature =
            RecoverableEcdsaSignature::new(update.signature_text(), signature.clone());
        let create = CreateInbox {
            nonce: 0,
            account_address: address,
            initial_address_signature: Box::new(ecdsa_signature),
        };

        (inbox_id, wallet, signing_key, create)
    }

    async fn generate_installation_association(
        signing_key: &SigningKey,
        wallet: LocalWallet,
        inbox_id: &str,
    ) -> AddAssociation {
        let keypair = SignatureKeyPair::from_raw(
            CIPHERSUITE.signature_algorithm(),
            signing_key.to_bytes().into(),
            signing_key.verifying_key().to_bytes().into(),
        );

        let action = UnsignedAction::AddAssociation(UnsignedAddAssociation {
            new_member_identifier: MemberIdentifier::Installation(keypair.public().to_vec()),
        });

        let update = UnsignedIdentityUpdate {
            client_timestamp_ns: 2_000_000u64,
            inbox_id: inbox_id.to_owned(),
            actions: vec![action],
        };

        let mut prehashed = Sha512::new();
        prehashed.update(update.signature_text());
        let signature = signing_key
            .sign_prehashed(prehashed.clone(), Some(INSTALLATION_KEY_SIGNATURE_CONTEXT))
            .unwrap();
        let existing_member = wallet
            .sign_message(update.signature_text())
            .await
            .unwrap()
            .to_vec();

        let existing_member =
            RecoverableEcdsaSignature::new(update.signature_text(), existing_member);

        let signature = InstallationKeySignature::new(
            update.signature_text(),
            signature.to_vec(),
            signing_key.verifying_key().as_bytes().to_vec(),
        );

        AddAssociation {
            new_member_signature: Box::new(signature),
            new_member_identifier: MemberIdentifier::Installation(keypair.public().to_vec()),
            existing_member_signature: Box::new(existing_member),
        }
    }

    fn build_key_package_bytes(
        keypair: &SignatureKeyPair,
        credential_with_key: &CredentialWithKey,
        account_address: Option<String>,
    ) -> Vec<u8> {
        let rust_crypto = OpenMlsRustCrypto::default();

        let kp = KeyPackage::builder();

        let kp = if let Some(address) = account_address {
            let application_id =
                Extension::ApplicationId(ApplicationIdExtension::new(address.as_bytes()));
            kp.leaf_node_extensions(Extensions::single(application_id))
        } else {
            kp
        };

        let kp = kp
            .build(
                CIPHERSUITE,
                &rust_crypto,
                keypair,
                credential_with_key.clone(),
            )
            .unwrap();
        kp.key_package().tls_serialize_detached().unwrap()
    }

    fn to_signature_keypair(key: SigningKey) -> SignatureKeyPair {
        let secret = key.to_bytes();
        let public = key.verifying_key().to_bytes();

        SignatureKeyPair::from_raw(
            CIPHERSUITE.signature_algorithm(),
            secret.into(),
            public.into(),
        )
    }

    // this test will panic until signature recovery is added
    // and `MockSignature` is updated with signatures that can be recovered
    #[tokio::test]
    #[should_panic]
    async fn test_get_association_state() {
        let create_request = CreateInbox::default();
        let inbox_id = generate_inbox_id(&create_request.account_address, &create_request.nonce);

        let updates = vec![IdentityUpdate::new_test(
            vec![Action::CreateInbox(create_request)],
            inbox_id.clone(),
        )];

        ValidationService::default()
            .get_association_state(Request::new(GetAssociationStateRequest {
                old_updates: vec![],
                new_updates: updates
                    .into_iter()
                    .map(IdentityUpdateProto::from)
                    .collect::<Vec<_>>(),
            }))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_validate_inbox_id_key_package_happy_path() {
        let (inbox_id, _, keypair, _) = generate_inbox_id_credential().await;
        let keypair = to_signature_keypair(keypair);

        let credential: OpenMlsCredential = InboxIdMlsCredential {
            inbox_id: inbox_id.clone(),
        }
        .try_into()
        .unwrap();

        let credential_with_key = CredentialWithKey {
            credential,
            signature_key: keypair.to_public_vec().into(),
        };

        let key_package_bytes = build_key_package_bytes(&keypair, &credential_with_key, None);
        let request = ValidateInboxIdKeyPackagesRequest {
            key_packages: vec![KeyPackageProtoWrapper {
                key_package_bytes_tls_serialized: key_package_bytes,
                is_inbox_id_credential: false,
            }],
        };

        let res = ValidationService::default()
            .validate_inbox_id_key_packages(Request::new(request))
            .await
            .unwrap();

        let first_response = &res.into_inner().responses[0];
        assert!(first_response.is_ok);
        assert_eq!(first_response.installation_public_key, keypair.public());
        assert_eq!(
            first_response.credential.as_ref().unwrap().inbox_id,
            inbox_id
        );
    }

    #[tokio::test]
    async fn test_validate_inbox_id_key_package_failure() {
        let (inbox_id, _, keypair, _) = generate_inbox_id_credential().await;
        let (_, _, other_keypair, _) = generate_inbox_id_credential().await;

        let keypair = to_signature_keypair(keypair);
        let other_keypair = to_signature_keypair(other_keypair);

        let credential: OpenMlsCredential = InboxIdMlsCredential {
            inbox_id: inbox_id.clone(),
        }
        .try_into()
        .unwrap();

        let credential_with_key = CredentialWithKey {
            credential,
            signature_key: other_keypair.to_public_vec().into(),
        };

        let key_package_bytes = build_key_package_bytes(&keypair, &credential_with_key, None);
        let request = ValidateInboxIdKeyPackagesRequest {
            key_packages: vec![KeyPackageProtoWrapper {
                key_package_bytes_tls_serialized: key_package_bytes,
                is_inbox_id_credential: false,
            }],
        };

        let res = ValidationService::default()
            .validate_inbox_id_key_packages(Request::new(request))
            .await
            .unwrap();

        let first_response = &res.into_inner().responses[0];
        assert!(!first_response.is_ok);
        assert_eq!(
            first_response.error_message,
            "XMTP Key Package failed mls validation: The leaf node signature is not valid."
        );
        assert_eq!(first_response.credential, None);
        assert_eq!(first_response.installation_public_key, Vec::<u8>::new());
    }
}
