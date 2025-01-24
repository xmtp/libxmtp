use ethers::types::{BlockNumber, U64};
use futures::future::{join_all, try_join_all};
use openmls::prelude::{tls_codec::Deserialize, MlsMessageIn, ProtocolMessage};
use openmls_rust_crypto::RustCrypto;
use tonic::{Request, Response, Status};

use xmtp_id::{
    associations::{
        self, try_map_vec, unverified::UnverifiedIdentityUpdate, AssociationError,
        DeserializationError, SignatureError,
    },
    scw_verifier::{SmartContractSignatureVerifier, ValidationResponse},
};
use xmtp_mls::{
    utils::id::serialize_group_id,
    verified_key_package_v2::{KeyPackageVerificationError, VerifiedKeyPackageV2},
};
use xmtp_proto::xmtp::{
    identity::{
        api::v1::{
            verify_smart_contract_wallet_signatures_response::ValidationResponse as VerifySmartContractWalletSignaturesValidationResponse,
            VerifySmartContractWalletSignatureRequestSignature,
            VerifySmartContractWalletSignaturesRequest,
            VerifySmartContractWalletSignaturesResponse,
        },
        associations::IdentityUpdate as IdentityUpdateProto,
    },
    mls_validation::v1::{
        validate_group_messages_response::ValidationResponse as ValidateGroupMessageValidationResponse,
        validate_inbox_id_key_packages_response::Response as ValidateInboxIdKeyPackageResponse,
        validation_api_server::ValidationApi,
        GetAssociationStateRequest,
        GetAssociationStateResponse,
        ValidateGroupMessagesRequest,
        ValidateGroupMessagesResponse,
        ValidateInboxIdKeyPackagesResponse,
        ValidateKeyPackagesRequest, // VerifySmartContractWalletSignaturesRequest, VerifySmartContractWalletSignaturesResponse,
    },
};

#[derive(Debug, thiserror::Error)]
pub enum GrpcServerError {
    #[error(transparent)]
    Deserialization(#[from] DeserializationError),
    #[error(transparent)]
    Association(#[from] AssociationError),
    #[error(transparent)]
    Signature(#[from] SignatureError),
}

impl From<GrpcServerError> for Status {
    fn from(err: GrpcServerError) -> Self {
        Status::invalid_argument(err.to_string())
    }
}

pub struct ValidationService {
    pub(crate) scw_verifier: Box<dyn SmartContractSignatureVerifier>,
}

impl ValidationService {
    pub fn new(scw_verifier: impl SmartContractSignatureVerifier + 'static) -> Self {
        Self {
            scw_verifier: Box::new(scw_verifier),
        }
    }
}

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

        get_association_state(old_updates, new_updates, &self.scw_verifier)
            .await
            .map(Response::new)
            .map_err(Into::into)
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: Request<VerifySmartContractWalletSignaturesRequest>,
    ) -> Result<Response<VerifySmartContractWalletSignaturesResponse>, Status> {
        let VerifySmartContractWalletSignaturesRequest { signatures } = request.into_inner();

        verify_smart_contract_wallet_signatures(signatures, &self.scw_verifier).await
    }

    async fn validate_inbox_id_key_packages(
        &self,
        request: Request<ValidateKeyPackagesRequest>,
    ) -> Result<Response<ValidateInboxIdKeyPackagesResponse>, Status> {
        let ValidateKeyPackagesRequest { key_packages } = request.into_inner();

        let responses: Vec<_> = key_packages
            .into_iter()
            .map(|k| k.key_package_bytes_tls_serialized)
            .map(validate_inbox_id_key_package)
            .collect();

        let responses: Vec<_> = join_all(responses)
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

async fn verify_smart_contract_wallet_signatures(
    signatures: Vec<VerifySmartContractWalletSignatureRequestSignature>,
    scw_verifier: impl SmartContractSignatureVerifier,
) -> Result<Response<VerifySmartContractWalletSignaturesResponse>, Status> {
    let mut responses = vec![];
    for signature in signatures {
        let verifier = &scw_verifier;
        let handle = async move {
            let account_id = signature.account_id.try_into().map_err(|_e| {
                GrpcServerError::Deserialization(DeserializationError::InvalidAccountId)
            })?;

            let response = verifier
                .is_valid_signature(
                    account_id,
                    signature.hash.try_into().map_err(|_| {
                        GrpcServerError::Deserialization(DeserializationError::InvalidHash)
                    })?,
                    signature.signature.into(),
                    signature
                        .block_number
                        .map(|bn| BlockNumber::Number(U64::from(bn))),
                )
                .await
                .map_err(|e| GrpcServerError::Signature(SignatureError::VerifierError(e)))?;

            Ok::<ValidationResponse, GrpcServerError>(response)
        };

        responses.push(handle);
    }

    let responses: Vec<_> = join_all(responses)
        .await
        .into_iter()
        .map(|result| match result {
            Err(err) => VerifySmartContractWalletSignaturesValidationResponse {
                is_valid: false,
                block_number: None,
                error: Some(format!("{err:?}")),
            },
            Ok(response) => VerifySmartContractWalletSignaturesValidationResponse {
                is_valid: response.is_valid,
                block_number: response.block_number,
                error: None,
            },
        })
        .collect();

    Ok(Response::new(VerifySmartContractWalletSignaturesResponse {
        responses,
    }))
}

async fn get_association_state(
    old_updates: Vec<IdentityUpdateProto>,
    new_updates: Vec<IdentityUpdateProto>,
    scw_verifier: impl SmartContractSignatureVerifier,
) -> Result<GetAssociationStateResponse, GrpcServerError> {
    let old_unverified_updates: Vec<UnverifiedIdentityUpdate> = try_map_vec(old_updates)?;
    let new_unverified_updates: Vec<UnverifiedIdentityUpdate> = try_map_vec(new_updates)?;

    let old_updates = try_join_all(
        old_unverified_updates
            .iter()
            .map(|u| u.to_verified(&scw_verifier)),
    )
    .await?;
    let new_updates = try_join_all(
        new_unverified_updates
            .iter()
            .map(|u| u.to_verified(&scw_verifier)),
    )
    .await?;
    if old_updates.is_empty() {
        let new_state = associations::get_state(&new_updates)?;
        return Ok(GetAssociationStateResponse {
            association_state: Some(new_state.clone().into()),
            state_diff: Some(new_state.as_diff().into()),
        });
    }

    let old_state = associations::get_state(&old_updates)?;
    let mut new_state = old_state.clone();
    for update in new_updates {
        new_state = associations::apply_update(new_state, update)?;
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
    use std::sync::Arc;

    use associations::AccountId;
    use ethers::{
        abi::Token,
        signers::{LocalWallet, Signer as _},
        types::{Bytes, H256, U256},
    };
    use openmls::{
        extensions::{ApplicationIdExtension, Extension, Extensions},
        key_packages::KeyPackage,
        prelude::{tls_codec::Serialize, Credential as OpenMlsCredential, CredentialWithKey},
    };
    use openmls_rust_crypto::OpenMlsRustCrypto;
    use xmtp_common::{rand_string, rand_u64};
    use xmtp_cryptography::XmtpInstallationCredential;
    use xmtp_id::{
        associations::{
            generate_inbox_id,
            test_utils::MockSmartContractSignatureVerifier,
            unverified::{UnverifiedAction, UnverifiedIdentityUpdate},
        },
        is_smart_contract,
        utils::test::{with_smart_contracts, CoinbaseSmartWallet},
    };
    use xmtp_mls::configuration::CIPHERSUITE;
    use xmtp_proto::xmtp::{
        identity::{
            associations::IdentityUpdate as IdentityUpdateProto,
            MlsCredential as InboxIdMlsCredential,
        },
        mls_validation::v1::validate_key_packages_request::KeyPackage as KeyPackageProtoWrapper,
    };

    use super::*;

    impl Default for ValidationService {
        fn default() -> Self {
            Self::new(MockSmartContractSignatureVerifier::new(true))
        }
    }

    fn generate_inbox_id_credential() -> (String, XmtpInstallationCredential) {
        let signing_key = XmtpInstallationCredential::new();

        let wallet = LocalWallet::new(&mut rand::thread_rng());
        let address = format!("0x{}", hex::encode(wallet.address()));

        let inbox_id = generate_inbox_id(&address, &0).unwrap();

        (inbox_id, signing_key)
    }

    fn build_key_package_bytes(
        keypair: &XmtpInstallationCredential,
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

    // this test will panic until signature recovery is added
    // and `MockSignature` is updated with signatures that can be recovered
    #[tokio::test]
    #[should_panic]
    async fn test_get_association_state() {
        let account_address = rand_string::<24>();
        let nonce = rand_u64();
        let inbox_id = generate_inbox_id(&account_address, &nonce).unwrap();
        let update = UnverifiedIdentityUpdate::new_test(
            vec![UnverifiedAction::new_test_create_inbox(
                &account_address,
                &nonce,
            )],
            inbox_id.clone(),
        );

        let updates = vec![update];

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
        let (inbox_id, keypair) = generate_inbox_id_credential();
        let credential: OpenMlsCredential = InboxIdMlsCredential {
            inbox_id: inbox_id.clone(),
        }
        .try_into()
        .unwrap();

        let credential_with_key = CredentialWithKey {
            credential,
            signature_key: keypair.public_slice().into(),
        };

        let key_package_bytes = build_key_package_bytes(&keypair, &credential_with_key, None);
        let request = ValidateKeyPackagesRequest {
            key_packages: vec![KeyPackageProtoWrapper {
                key_package_bytes_tls_serialized: key_package_bytes,
                is_inbox_id_credential: false,
            }],
        };

        let res = ValidationService::default()
            .validate_inbox_id_key_packages(Request::new(request))
            .await
            .unwrap();

        let res = res.into_inner();
        let first_response = &res.responses[0];
        assert!(first_response.is_ok);
        assert_eq!(
            first_response.installation_public_key,
            keypair.public_bytes()
        );
        assert_eq!(
            first_response.credential.as_ref().unwrap().inbox_id,
            inbox_id
        );
    }

    #[tokio::test]
    async fn test_validate_inbox_id_key_package_failure() {
        let (inbox_id, keypair) = generate_inbox_id_credential();
        let (_, other_keypair) = generate_inbox_id_credential();

        let credential: OpenMlsCredential = InboxIdMlsCredential {
            inbox_id: inbox_id.clone(),
        }
        .try_into()
        .unwrap();

        let credential_with_key = CredentialWithKey {
            credential,
            signature_key: other_keypair.public_slice().into(),
        };

        let key_package_bytes = build_key_package_bytes(&keypair, &credential_with_key, None);
        let request = ValidateKeyPackagesRequest {
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

    #[tokio::test]
    async fn test_validate_scw() {
        with_smart_contracts(|anvil, _provider, client, smart_contracts| async move {
            let key = anvil.keys()[0].clone();
            let wallet: LocalWallet = key.clone().into();

            let owners = vec![Bytes::from(H256::from(wallet.address()).0.to_vec())];

            let scw_factory = smart_contracts.coinbase_smart_wallet_factory();
            let nonce = U256::from(0);

            let scw_addr = scw_factory
                .get_address(owners.clone(), nonce)
                .await
                .unwrap();

            let contract_call = scw_factory.create_account(owners.clone(), nonce);
            contract_call.send().await.unwrap().await.unwrap();

            assert!(is_smart_contract(scw_addr, anvil.endpoint(), None)
                .await
                .unwrap());

            let hash = H256::random().into();
            let smart_wallet = CoinbaseSmartWallet::new(
                scw_addr,
                Arc::new(client.with_signer(wallet.clone().with_chain_id(anvil.chain_id()))),
            );
            let replay_safe_hash = smart_wallet.replay_safe_hash(hash).call().await.unwrap();
            let account_id = AccountId::new_evm(anvil.chain_id(), format!("{scw_addr:?}"));

            let signature = ethers::abi::encode(&[Token::Tuple(vec![
                Token::Uint(U256::from(0)),
                Token::Bytes(wallet.sign_hash(replay_safe_hash.into()).unwrap().to_vec()),
            ])]);

            let resp = ValidationService::default()
                .verify_smart_contract_wallet_signatures(Request::new(
                    VerifySmartContractWalletSignaturesRequest {
                        signatures: vec![VerifySmartContractWalletSignatureRequestSignature {
                            account_id: account_id.into(),
                            block_number: None,
                            hash: hash.to_vec(),
                            signature,
                        }],
                    },
                ))
                .await
                .unwrap();

            let VerifySmartContractWalletSignaturesResponse { responses } = resp.into_inner();

            assert_eq!(responses.len(), 1);
            assert_eq!(
                responses[0],
                VerifySmartContractWalletSignaturesValidationResponse {
                    is_valid: true,
                    block_number: Some(1),
                    error: None
                }
            );
        })
        .await;
    }
}
