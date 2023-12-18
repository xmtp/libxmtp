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

use crate::validation_helpers::identity_to_wallet_address;

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
                        wallet_address: res.wallet_address,
                        error_message: "".to_string(),
                        is_ok: true,
                    },
                    Err(e) => ValidateKeyPackageValidationResponse {
                        is_ok: false,
                        error_message: e,
                        credential_identity_bytes: vec![],
                        installation_id: vec![],
                        wallet_address: "".to_string(),
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
    wallet_address: String,
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
    let wallet_address = identity_to_wallet_address(identity_bytes, pub_key_bytes)?;

    Ok(ValidateKeyPackageResult {
        installation_id: pub_key_bytes.to_vec(),
        wallet_address,
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
        association::{AssociationContext, AssociationText, Eip191Association},
        InboxOwner,
    };
    use xmtp_proto::xmtp::{
        mls::message_contents::Eip191Association as Eip191AssociationProto,
        mls_validation::v1::validate_key_packages_request::KeyPackage as KeyPackageProtoWrapper,
    };

    use super::*;

    const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

    fn generate_identity() -> (Vec<u8>, SignatureKeyPair, String) {
        let rng = &mut rand::thread_rng();
        let wallet = LocalWallet::new(rng);
        let signature_key_pair = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).unwrap();
        let pub_key = signature_key_pair.public();
        let wallet_address = wallet.get_address();
        let association_text = AssociationText::new_static(
            AssociationContext::GrantMessagingAccess,
            wallet_address.clone(),
            pub_key.to_vec(),
            "2021-01-01T00:00:00Z".to_string(),
        );
        let signature = wallet
            .sign(&association_text.text())
            .expect("failed to sign");

        let association =
            Eip191Association::new(pub_key, association_text, signature).expect("bad signature");
        let association_proto: Eip191AssociationProto = association.into();
        let mut buf = Vec::new();
        association_proto
            .encode(&mut buf)
            .expect("failed to serialize");

        (buf, signature_key_pair, wallet_address)
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
        let (identity, keypair, wallet_address) = generate_identity();

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
        assert_eq!(first_response.wallet_address, wallet_address);
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
        assert_eq!(first_response.wallet_address, "".to_string());
    }
}
