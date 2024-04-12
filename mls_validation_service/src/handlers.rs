use openmls::{
    credentials::BasicCredential,
    prelude::{tls_codec::Deserialize, MlsMessageIn, ProtocolMessage},
};
use openmls_rust_crypto::RustCrypto;
use tonic::{Code, Request, Response, Status};

use xmtp_id::associations::{self, IdentityUpdate};
use xmtp_mls::{utils::id::serialize_group_id, verified_key_package::VerifiedKeyPackage};
use xmtp_proto::xmtp::mls_validation::v1::{
    validate_group_messages_response::ValidationResponse as ValidateGroupMessageValidationResponse,
    validate_key_packages_response::ValidationResponse as ValidateKeyPackageValidationResponse,
    validation_api_server::ValidationApi, GetAssociationStateRequest, GetAssociationStateResponse,
    ValidateGroupMessagesRequest, ValidateGroupMessagesResponse, ValidateKeyPackagesRequest,
    ValidateKeyPackagesResponse,
};

#[derive(Debug, Default)]
pub struct ValidationService {}

#[tonic::async_trait]
impl ValidationApi for ValidationService {
    async fn validate_key_packages(
        &self,
        request: Request<ValidateKeyPackagesRequest>,
    ) -> Result<Response<ValidateKeyPackagesResponse>, Status> {
        let out: Vec<ValidateKeyPackageValidationResponse> = request
            .into_inner()
            .key_packages
            .into_iter()
            .map(
                |kp| match validate_key_package(kp.key_package_bytes_tls_serialized) {
                    Ok(res) => ValidateKeyPackageValidationResponse {
                        is_ok: true,
                        error_message: "".to_string(),
                        installation_id: res.installation_id,
                        account_address: res.account_address,
                        credential_identity_bytes: res.credential_identity_bytes,
                        expiration: res.expiration,
                    },
                    Err(e) => ValidateKeyPackageValidationResponse {
                        is_ok: false,
                        error_message: e,
                        installation_id: vec![],
                        account_address: "".to_string(),
                        credential_identity_bytes: vec![],
                        expiration: 0,
                    },
                },
            )
            .collect();

        Ok(Response::new(ValidateKeyPackagesResponse {
            responses: out,
        }))
    }

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
        let updates = request
            .into_inner()
            .updates
            .into_iter()
            .map(IdentityUpdate::from_proto)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Status::new(Code::Cancelled, e.to_string()))?;

        let state = associations::get_state(updates)
            .map_err(|e| Status::new(Code::Cancelled, e.to_string()))?;

        Ok(Response::new(GetAssociationStateResponse {
            association_state: Some(state.into()),
        }))
    }
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

struct ValidateKeyPackageResult {
    installation_id: Vec<u8>,
    account_address: String,
    credential_identity_bytes: Vec<u8>,
    expiration: u64,
}

fn validate_key_package(key_package_bytes: Vec<u8>) -> Result<ValidateKeyPackageResult, String> {
    let rust_crypto = RustCrypto::default();
    let verified_key_package =
        VerifiedKeyPackage::from_bytes(&rust_crypto, key_package_bytes.as_slice())
            .map_err(|e| e.to_string())?;

    let credential = verified_key_package.inner.leaf_node().credential();

    let basic_credential = BasicCredential::try_from(credential).map_err(|e| e.to_string())?;

    Ok(ValidateKeyPackageResult {
        installation_id: verified_key_package.installation_id(),
        account_address: verified_key_package.account_address,
        credential_identity_bytes: basic_credential.identity().to_vec(),
        expiration: verified_key_package.inner.life_time().not_after(),
    })
}

#[cfg(test)]
mod tests {
    use ethers::signers::LocalWallet;
    use openmls::{
        extensions::{ApplicationIdExtension, Extension, Extensions},
        prelude::{
            tls_codec::Serialize, Ciphersuite, Credential as OpenMlsCredential, CredentialWithKey,
            CryptoConfig,
        },
        prelude_test::KeyPackage,
        versions::ProtocolVersion,
    };
    use openmls_basic_credential::SignatureKeyPair;
    use openmls_rust_crypto::OpenMlsRustCrypto;
    use prost::Message;
    use xmtp_mls::{credential::Credential, InboxOwner};
    use xmtp_proto::xmtp::{
        mls::message_contents::MlsCredential as CredentialProto,
        mls_validation::v1::validate_key_packages_request::KeyPackage as KeyPackageProtoWrapper,
    };

    use super::*;

    const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

    fn generate_identity() -> (Vec<u8>, SignatureKeyPair, String) {
        let rng = &mut rand::thread_rng();
        let wallet = LocalWallet::new(rng);
        let signature_key_pair = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).unwrap();
        let _pub_key = signature_key_pair.public();
        let account_address = wallet.get_address();

        let credential =
            Credential::create(&signature_key_pair, &wallet).expect("failed to create credential");
        let credential_proto: CredentialProto = credential.into();

        (
            credential_proto.encode_to_vec(),
            signature_key_pair,
            account_address,
        )
    }

    fn build_key_package_bytes(
        keypair: &SignatureKeyPair,
        credential_with_key: &CredentialWithKey,
        account_address: String,
    ) -> Vec<u8> {
        let rust_crypto = OpenMlsRustCrypto::default();
        let application_id =
            Extension::ApplicationId(ApplicationIdExtension::new(account_address.as_bytes()));

        let kp = KeyPackage::builder()
            .leaf_node_extensions(Extensions::single(application_id))
            .build(
                CryptoConfig {
                    ciphersuite: CIPHERSUITE,
                    version: ProtocolVersion::default(),
                },
                &rust_crypto,
                keypair,
                credential_with_key.clone(),
            )
            .unwrap();
        kp.tls_serialize_detached().unwrap()
    }

    #[tokio::test]
    async fn test_validate_key_packages_happy_path() {
        let (identity, keypair, account_address) = generate_identity();

        let credential: OpenMlsCredential = BasicCredential::new(identity).unwrap().into();
        let credential_with_key = CredentialWithKey {
            credential,
            signature_key: keypair.to_public_vec().into(),
        };

        let key_package_bytes =
            build_key_package_bytes(&keypair, &credential_with_key, account_address.clone());
        let request = ValidateKeyPackagesRequest {
            key_packages: vec![KeyPackageProtoWrapper {
                key_package_bytes_tls_serialized: key_package_bytes,
            }],
        };

        let res = ValidationService::default()
            .validate_key_packages(Request::new(request))
            .await
            .unwrap();

        let first_response = &res.into_inner().responses[0];
        assert_eq!(first_response.installation_id, keypair.public());
        assert_eq!(first_response.account_address, account_address);
        assert!(!first_response.credential_identity_bytes.is_empty());
    }

    #[tokio::test]
    async fn test_validate_key_packages_fail() {
        let (identity, keypair, account_address) = generate_identity();
        let (_, other_keypair, _) = generate_identity();

        let credential: OpenMlsCredential = BasicCredential::new(identity).unwrap().into();
        let credential_with_key = CredentialWithKey {
            credential,
            // Use the wrong signature key to make the validation fail
            signature_key: other_keypair.to_public_vec().into(),
        };

        let key_package_bytes =
            build_key_package_bytes(&keypair, &credential_with_key, account_address);

        let request = ValidateKeyPackagesRequest {
            key_packages: vec![KeyPackageProtoWrapper {
                key_package_bytes_tls_serialized: key_package_bytes,
            }],
        };

        let res = ValidationService::default()
            .validate_key_packages(Request::new(request))
            .await
            .unwrap();

        let first_response = &res.into_inner().responses[0];

        assert!(!first_response.is_ok);
        assert_eq!(first_response.account_address, "".to_string());
    }
}
