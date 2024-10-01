use ethers::types::{BlockNumber, U64};
use futures::future::{join_all, try_join_all};
use openmls::prelude::{tls_codec::Deserialize, BasicCredential, MlsMessageIn, ProtocolMessage};
use openmls_rust_crypto::RustCrypto;
use tonic::{Request, Response, Status};

use xmtp_id::{
    associations::{
        self, try_map_vec, unverified::UnverifiedIdentityUpdate, AccountId, AssociationError,
        DeserializationError, IdentityUpdate, MemberIdentifier, SignatureError,
    },
    scw_verifier::SmartContractSignatureVerifier,
};
use xmtp_mls::{
    utils::id::serialize_group_id,
    verified_key_package::VerifiedKeyPackage,
    verified_key_package_v2::{KeyPackageVerificationError, VerifiedKeyPackageV2},
};
use xmtp_proto::xmtp::{
    identity::associations::{IdentityUpdate as IdentityUpdateProto, SmartContractWalletSignature},
    mls_validation::v1::{
        validate_group_messages_response::ValidationResponse as ValidateGroupMessageValidationResponse,
        validate_inbox_id_key_packages_response::Response as ValidateInboxIdKeyPackageResponse,
        validate_inbox_ids_request::ValidationRequest as InboxIdValidationRequest,
        validate_inbox_ids_response::ValidationResponse as InboxIdValidationResponse,
        validate_key_packages_response::ValidationResponse as ValidateKeyPackagesValidationResponse,
        validate_key_packages_response::ValidationResponse as ValidateKeyPackageValidationResponse,
        validation_api_server::ValidationApi, GetAssociationStateRequest,
        GetAssociationStateResponse, ValidateGroupMessagesRequest, ValidateGroupMessagesResponse,
        ValidateInboxIdKeyPackagesResponse, ValidateInboxIdsRequest, ValidateInboxIdsResponse,
        ValidateKeyPackagesRequest, ValidateKeyPackagesResponse,
        VerifySmartContractWalletSignaturesRequest, VerifySmartContractWalletSignaturesResponse,
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
    pub fn new(scw_verifier: impl SmartContractSignatureVerifier) -> Self {
        Self {
            scw_verifier: Box::new(scw_verifier),
        }
    }
}

#[tonic::async_trait]
impl ValidationApi for ValidationService {
    async fn validate_inbox_ids(
        &self,
        request: tonic::Request<ValidateInboxIdsRequest>,
    ) -> Result<tonic::Response<ValidateInboxIdsResponse>, tonic::Status> {
        let ValidateInboxIdsRequest { requests } = request.into_inner();
        let responses: Vec<_> = requests
            .into_iter()
            .map(|r| validate_inbox_id(r, &*self.scw_verifier))
            .collect();

        let responses: Vec<InboxIdValidationResponse> = join_all(responses)
            .await
            .into_iter()
            .map(|res| res.map_err(InboxIdValidationResponse::from))
            .map(|r| r.unwrap_or_else(|e| e))
            .collect();
        Ok(Response::new(ValidateInboxIdsResponse { responses }))
    }

    async fn validate_key_packages(
        &self,
        request: tonic::Request<ValidateKeyPackagesRequest>,
    ) -> std::result::Result<tonic::Response<ValidateKeyPackagesResponse>, tonic::Status> {
        let out: Vec<ValidateKeyPackagesValidationResponse> = request
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
        let GetAssociationStateRequest {
            old_updates,
            new_updates,
        } = request.into_inner();

        get_association_state(old_updates, new_updates, self.scw_verifier.as_ref())
            .await
            .map(Response::new)
            .map_err(Into::into)
    }

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: Request<VerifySmartContractWalletSignaturesRequest>,
    ) -> Result<Response<VerifySmartContractWalletSignaturesResponse>, Status> {
        let VerifySmartContractWalletSignaturesRequest { signatures } = request.into_inner();

        verify_smart_contract_wallet_signatures(signatures, self.scw_verifier.as_ref()).await
    }

    async fn validate_inbox_id_key_packages(
        &self,
        request: tonic::Request<ValidateKeyPackagesRequest>,
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

/// Error type for inbox ID validation
/// Each variant requires carrying the ID that failed to validate
/// The error variant itself becomes the failed version of `InboxIdValidationResponse` but allows
/// us to write normal rust in `validate_inbox_id`
#[derive(thiserror::Error, Debug)]
enum InboxIdValidationError {
    #[error("Inbox ID {id} failed to validate")]
    Deserialization {
        id: String,
        source: DeserializationError,
    },
    #[error("Valid association state could not be found for inbox {id}, {source}")]
    Association {
        id: String,
        source: AssociationError,
    },
    #[error("Missing Credential")]
    MissingCredential,
    #[error("Inbox {id} is not associated with member {member}")]
    MemberNotAssociated {
        id: String,
        member: MemberIdentifier,
    },
    #[error(
        "Given Inbox Id, {credential_inbox_id} does not match resulting inbox id, {state_inbox_id}"
    )]
    InboxIdDoesNotMatch {
        credential_inbox_id: String,
        state_inbox_id: String,
    },
}

impl InboxIdValidationError {
    pub fn inbox_id(&self) -> String {
        match self {
            InboxIdValidationError::Deserialization { id, .. } => id.clone(),
            InboxIdValidationError::MissingCredential => "null".to_string(),
            InboxIdValidationError::Association { id, .. } => id.clone(),
            InboxIdValidationError::MemberNotAssociated { id, .. } => id.clone(),
            InboxIdValidationError::InboxIdDoesNotMatch {
                credential_inbox_id,
                ..
            } => credential_inbox_id.clone(),
        }
    }
}

impl From<InboxIdValidationError> for InboxIdValidationResponse {
    fn from(err: InboxIdValidationError) -> Self {
        InboxIdValidationResponse {
            is_ok: false,
            error_message: err.to_string(),
            inbox_id: err.inbox_id(),
        }
    }
}

async fn validate_inbox_id(
    request: InboxIdValidationRequest,
    scw_verifier: &dyn SmartContractSignatureVerifier,
) -> Result<InboxIdValidationResponse, InboxIdValidationError> {
    let InboxIdValidationRequest {
        credential,
        installation_public_key,
        identity_updates,
    } = request;

    if credential.is_none() {
        return Err(InboxIdValidationError::MissingCredential);
    }

    let inbox_id = credential.expect("checked for empty credential").inbox_id;

    let unverified_identity_updates: Vec<UnverifiedIdentityUpdate> = try_map_vec(identity_updates)
        .map_err(|e| InboxIdValidationError::Deserialization {
            source: e,
            id: inbox_id.clone(),
        })?;
    let identity_updates: Vec<IdentityUpdate> = try_join_all(
        unverified_identity_updates
            .iter()
            .map(|u| u.to_verified(scw_verifier))
            .collect::<Vec<_>>(),
    )
    .await
    .unwrap();

    let state = associations::get_state(identity_updates).map_err(|e| {
        InboxIdValidationError::Association {
            source: e,
            id: inbox_id.clone(),
        }
    })?;

    // this is defensive and should not happen.
    // The only way an inbox id is different is if xmtp-node-go hands over identity updates with a different inbox id.
    // which is a bug.
    if state.inbox_id().as_ref() != *inbox_id {
        return Err(InboxIdValidationError::InboxIdDoesNotMatch {
            credential_inbox_id: inbox_id.clone(),
            state_inbox_id: state.inbox_id().clone(),
        });
    }

    let member = MemberIdentifier::Installation(installation_public_key);
    if state.get(&member).is_none() {
        return Err(InboxIdValidationError::MemberNotAssociated {
            id: inbox_id,
            member,
        });
    }
    Ok(InboxIdValidationResponse {
        is_ok: true,
        error_message: "".to_string(),
        inbox_id,
    })
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
    signatures: Vec<SmartContractWalletSignature>,
    scw_verifier: &dyn SmartContractSignatureVerifier,
) -> Result<Response<VerifySmartContractWalletSignaturesResponse>, Status> {
    let mut futures = vec![];

    for sig in signatures {
        futures.push(scw_verifier.is_valid_signature(
            AccountId::new_evm(sig.chain_id, sig.account_id),
            [0; 32],
            sig.signature.into(),
            Some(BlockNumber::Number(U64([sig.block_number]))),
        ));
    }

    Ok(Response::new(VerifySmartContractWalletSignaturesResponse {
        responses: vec![],
    }))
}

async fn get_association_state(
    old_updates: Vec<IdentityUpdateProto>,
    new_updates: Vec<IdentityUpdateProto>,
    scw_verifier: &dyn SmartContractSignatureVerifier,
) -> Result<GetAssociationStateResponse, GrpcServerError> {
    let old_unverified_updates: Vec<UnverifiedIdentityUpdate> = try_map_vec(old_updates)?;
    let new_unverified_updates: Vec<UnverifiedIdentityUpdate> = try_map_vec(new_updates)?;

    let old_updates = try_join_all(
        old_unverified_updates
            .iter()
            .map(|u| u.to_verified(scw_verifier)),
    )
    .await?;
    let new_updates = try_join_all(
        new_unverified_updates
            .iter()
            .map(|u| u.to_verified(scw_verifier)),
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

    let basic_credential =
        BasicCredential::try_from(credential.clone()).map_err(|e| e.to_string())?;

    Ok(ValidateKeyPackageResult {
        installation_id: verified_key_package.installation_id(),
        account_address: verified_key_package.account_address,
        credential_identity_bytes: basic_credential.identity().to_vec(),
        expiration: 0,
    })
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::SigningKey;
    use ethers::signers::{LocalWallet, Signer as _};
    use openmls::{
        extensions::{ApplicationIdExtension, Extension, Extensions},
        prelude::{tls_codec::Serialize, Credential as OpenMlsCredential, CredentialWithKey},
        prelude_test::KeyPackage,
    };
    use openmls_basic_credential::SignatureKeyPair;
    use openmls_rust_crypto::OpenMlsRustCrypto;
    use xmtp_id::{
        associations::{
            generate_inbox_id,
            test_utils::{rand_string, rand_u64, MockSmartContractSignatureVerifier},
            unverified::{UnverifiedAction, UnverifiedIdentityUpdate},
        },
        InboxOwner,
    };
    use xmtp_mls::{configuration::CIPHERSUITE, credential::Credential};
    use xmtp_proto::xmtp::{
        identity::associations::IdentityUpdate as IdentityUpdateProto,
        identity::MlsCredential as InboxIdMlsCredential,
        mls::message_contents::MlsCredential as CredentialProto,
        mls_validation::v1::validate_key_packages_request::KeyPackage as KeyPackageProtoWrapper,
    };

    use prost::Message;

    use super::*;

    impl Default for ValidationService {
        fn default() -> Self {
            Self::new(MockSmartContractSignatureVerifier::new(true))
        }
    }

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

    async fn generate_inbox_id_credential() -> (String, SigningKey) {
        let signing_key = SigningKey::generate(&mut rand::thread_rng());

        let wallet = LocalWallet::new(&mut rand::thread_rng());
        let address = format!("0x{}", hex::encode(wallet.address()));

        let inbox_id = generate_inbox_id(&address, &0);

        (inbox_id, signing_key)
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

    #[tokio::test]
    async fn test_validate_key_packages_happy_path() {
        let (identity, keypair, account_address) = generate_identity();

        let credential: OpenMlsCredential = BasicCredential::new(identity).into();
        let credential_with_key = CredentialWithKey {
            credential,
            signature_key: keypair.to_public_vec().into(),
        };

        let key_package_bytes = build_key_package_bytes(
            &keypair,
            &credential_with_key,
            Some(account_address.clone()),
        );
        let request = ValidateKeyPackagesRequest {
            key_packages: vec![KeyPackageProtoWrapper {
                key_package_bytes_tls_serialized: key_package_bytes,
                is_inbox_id_credential: false,
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

        let credential: OpenMlsCredential = BasicCredential::new(identity).into();
        let credential_with_key = CredentialWithKey {
            credential,
            // Use the wrong signature key to make the validation fail
            signature_key: other_keypair.to_public_vec().into(),
        };

        let key_package_bytes =
            build_key_package_bytes(&keypair, &credential_with_key, Some(account_address));

        let request = ValidateKeyPackagesRequest {
            key_packages: vec![KeyPackageProtoWrapper {
                key_package_bytes_tls_serialized: key_package_bytes,
                is_inbox_id_credential: false,
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

    // this test will panic until signature recovery is added
    // and `MockSignature` is updated with signatures that can be recovered
    #[tokio::test]
    #[should_panic]
    async fn test_get_association_state() {
        let account_address = rand_string();
        let nonce = rand_u64();
        let inbox_id = generate_inbox_id(&account_address, &nonce);
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
        let (inbox_id, keypair) = generate_inbox_id_credential().await;
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
        assert!(first_response.is_ok);
        assert_eq!(first_response.installation_public_key, keypair.public());
        assert_eq!(
            first_response.credential.as_ref().unwrap().inbox_id,
            inbox_id
        );
    }

    #[tokio::test]
    async fn test_validate_inbox_id_key_package_failure() {
        let (inbox_id, keypair) = generate_inbox_id_credential().await;
        let (_, other_keypair) = generate_inbox_id_credential().await;

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
}
