use openmls::{
    prelude::{KeyPackageIn, MlsMessageIn, ProtocolMessage, TlsDeserializeTrait},
    versions::ProtocolVersion,
};
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::OpenMlsProvider;
use tonic::{Request, Response, Status};
use xmtp_mls::utils::id::serialize_group_id;
use xmtp_proto::xmtp::mls_validation::v1::{
    validate_group_messages_response::ValidationResponse as ValidateGroupMessageValidationResponse,
    validate_key_packages_response::ValidationResponse as ValidateKeyPackageValidationResponse,
    validation_api_server::ValidationApi, ValidateGroupMessagesRequest,
    ValidateGroupMessagesResponse, ValidateKeyPackagesRequest, ValidateKeyPackagesResponse,
};

use crate::validation_helpers::identity_to_account_address;

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
                        installation_id: res.installation_id,
                        credential_identity_bytes: res.credential_identity_bytes,
                        account_address: res.account_address,
                        error_message: "".to_string(),
                        is_ok: true,
                    },
                    Err(e) => ValidateKeyPackageValidationResponse {
                        is_ok: false,
                        error_message: e,
                        credential_identity_bytes: vec![],
                        installation_id: vec![],
                        account_address: "".to_string(),
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
}

struct ValidateGroupMessageResult {
    group_id: String,
}

fn validate_group_message(message: Vec<u8>) -> Result<ValidateGroupMessageResult, String> {
    let msg_result = MlsMessageIn::tls_deserialize(&mut message.as_slice())
        .map_err(|_| "failed to decode".to_string())?;

    let private_message: ProtocolMessage = msg_result.into();

    Ok(ValidateGroupMessageResult {
        group_id: serialize_group_id(private_message.group_id().as_slice()),
    })
}

struct ValidateKeyPackageResult {
    installation_id: Vec<u8>,
    account_address: String,
    credential_identity_bytes: Vec<u8>,
}

fn validate_key_package(key_package_bytes: Vec<u8>) -> Result<ValidateKeyPackageResult, String> {
    let deserialize_result = KeyPackageIn::tls_deserialize_bytes(key_package_bytes.as_slice())
        .map_err(|e| format!("deserialization error: {}", e))?;
    let rust_crypto = OpenMlsRustCrypto::default();
    let crypto = rust_crypto.crypto();

    // Validate the key package and check all signatures
    let kp = deserialize_result
        .clone()
        .validate(crypto, ProtocolVersion::Mls10)
        .map_err(|e| format!("validation failed: {}", e))?;

    // Get the credential so we can
    let leaf_node = kp.leaf_node();
    let identity_bytes = leaf_node.credential().identity();
    let pub_key_bytes = leaf_node.signature_key().as_slice();
    let account_address = identity_to_account_address(identity_bytes, pub_key_bytes)?;

    Ok(ValidateKeyPackageResult {
        installation_id: pub_key_bytes.to_vec(),
        account_address,
        credential_identity_bytes: identity_bytes.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use ethers::signers::LocalWallet;
    use openmls::{
        prelude::{
            Ciphersuite, Credential as OpenMlsCredential, CredentialType, CredentialWithKey,
            CryptoConfig, TlsSerializeTrait,
        },
        prelude_test::KeyPackage,
    };
    use openmls_basic_credential::SignatureKeyPair;
    use prost::Message;
    use xmtp_mls::{
        association::{AssociationContext, Credential},
        InboxOwner,
    };
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
        let pub_key = signature_key_pair.public();
        let account_address = wallet.get_address();

        let credential = Credential::create_eip191(&signature_key_pair, &wallet)
            .expect("failed to create credential");
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
    ) -> Vec<u8> {
        let rust_crypto = OpenMlsRustCrypto::default();
        let kp = KeyPackage::builder()
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

        let credential = OpenMlsCredential::new(identity, CredentialType::Basic).unwrap();
        let credential_with_key = CredentialWithKey {
            credential,
            signature_key: keypair.to_public_vec().into(),
        };

        let key_package_bytes = build_key_package_bytes(&keypair, &credential_with_key);
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
        let (identity, keypair, _) = generate_identity();
        let (_, other_keypair, _) = generate_identity();

        let credential = OpenMlsCredential::new(identity, CredentialType::Basic).unwrap();
        let credential_with_key = CredentialWithKey {
            credential,
            // Use the wrong signature key to make the validation fail
            signature_key: other_keypair.to_public_vec().into(),
        };

        let key_package_bytes = build_key_package_bytes(&keypair, &credential_with_key);

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
