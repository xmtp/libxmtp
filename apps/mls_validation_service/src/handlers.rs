use futures::future::join_all;
use tonic::{Code, Request, Response, Status, metadata::MetadataValue};

use xmtp_common::{ErrorCode, RetryableError};
use xmtp_id::key_package::KeyPackageVerificationError;
use xmtp_id::{
    associations::{AssociationError, DeserializationError, SignatureError},
    scw_verifier::{SmartContractSignatureVerifier, ValidationResponse},
};
use xmtp_mls_validation::{
    ValidationError, is_commit_or_proposal, parse_group_message, validate_identity_updates,
    verify_key_package,
};
use xmtp_proto::xmtp::{
    identity::{
        api::v1::{
            VerifySmartContractWalletSignatureRequestSignature,
            VerifySmartContractWalletSignaturesRequest,
            VerifySmartContractWalletSignaturesResponse,
            verify_smart_contract_wallet_signatures_response::ValidationResponse as VerifySmartContractWalletSignaturesValidationResponse,
        },
        associations::IdentityUpdate as IdentityUpdateProto,
    },
    mls_validation::v1::{
        GetAssociationStateRequest,
        GetAssociationStateResponse,
        ValidateGroupMessagesRequest,
        ValidateGroupMessagesResponse,
        ValidateInboxIdKeyPackagesResponse,
        ValidateKeyPackagesRequest, // VerifySmartContractWalletSignaturesRequest, VerifySmartContractWalletSignaturesResponse,
        validate_group_messages_response::ValidationResponse as ValidateGroupMessageValidationResponse,
        validate_inbox_id_key_packages_response::Response as ValidateInboxIdKeyPackageResponse,
        validation_api_server::ValidationApi,
    },
};

#[derive(Debug, thiserror::Error, ErrorCode)]
pub enum GrpcServerError {
    #[error(transparent)]
    #[error_code(inherit)]
    Deserialization(#[from] DeserializationError),
    #[error(transparent)]
    #[error_code(inherit)]
    Association(#[from] AssociationError),
    #[error(transparent)]
    #[error_code(inherit)]
    Signature(#[from] SignatureError),
    #[error(transparent)]
    #[error_code(inherit)]
    Conversion(#[from] xmtp_proto::ConversionError),
    #[error(transparent)]
    #[error_code(inherit)]
    Validation(#[from] ValidationError),
}

impl RetryableError for GrpcServerError {
    fn is_retryable(&self) -> bool {
        match self {
            GrpcServerError::Signature(e) => e.is_retryable(),
            // An association can fail because verifying one of its signatures hit a
            // transient error (e.g. the chain RPC), which should still be retryable.
            GrpcServerError::Association(AssociationError::Signature(e)) => e.is_retryable(),
            GrpcServerError::Conversion(e) => e.is_retryable(),
            GrpcServerError::Validation(e) => e.is_retryable(),
            GrpcServerError::Deserialization(_) | GrpcServerError::Association(_) => false,
        }
    }
}

impl From<GrpcServerError> for Status {
    fn from(err: GrpcServerError) -> Self {
        // Retryable errors are transient and server-side (for example, failing to
        // reach the chain RPC while verifying a smart contract wallet signature), so
        // surface them as `Unavailable` and let callers retry. Everything else is a
        // genuine bad request and stays `InvalidArgument`.
        let code = if err.is_retryable() {
            Code::Unavailable
        } else {
            Code::InvalidArgument
        };

        let mut status = Status::new(code, err.to_string());
        // Attach the stable error label (e.g. "SignatureError::VerifierError") as
        // metadata so callers get a precise reason instead of only a free-form string.
        if let Ok(value) = MetadataValue::try_from(err.error_code()) {
            status.metadata_mut().insert("error-code", value);
        }
        status
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
            .map(
                |message| match parse_group_message(&message.group_message_bytes_tls_serialized) {
                    Ok(res) => ValidateGroupMessageValidationResponse {
                        group_id: hex::encode(res.group_id().as_slice()),
                        error_message: "".to_string(),
                        is_ok: true,
                        is_commit: is_commit_or_proposal(&res),
                    },
                    Err(e) => ValidateGroupMessageValidationResponse {
                        group_id: "".to_string(),
                        error_message: e.to_string(),
                        is_ok: false,
                        is_commit: false,
                    },
                },
            )
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
    let kp = verify_key_package(&key_package)?;

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
                    signature.block_number,
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
    let result = validate_identity_updates(old_updates, new_updates, scw_verifier).await?;
    Ok(GetAssociationStateResponse {
        association_state: Some(result.state.into()),
        state_diff: Some(result.diff.into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::dyn_abi::SolType;
    use alloy::primitives::{B256, U256};
    use alloy::providers::Provider;
    use alloy::signers::Signer;
    use xmtp_id::associations::{AccountId, test_utils::MockSmartContractSignatureVerifier};
    use xmtp_id::utils::test::{SignatureWithNonce, SmartWalletContext, docker_smart_wallet};

    impl Default for ValidationService {
        fn default() -> Self {
            Self::new(MockSmartContractSignatureVerifier::new(true))
        }
    }

    #[rstest::rstest]
    #[xmtp_common::timeout(std::time::Duration::from_secs(30))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_validate_scw(#[future] docker_smart_wallet: SmartWalletContext) {
        let SmartWalletContext {
            owner0: wallet,
            factory,
            sw,
            sw_address,
            ..
        } = docker_smart_wallet.await;

        let provider = factory.provider();
        let chain_id = provider.get_chain_id().await.unwrap();
        let hash = B256::random();
        let account_id = AccountId::new_evm(chain_id, format!("{sw_address}"));
        let rsh = sw.replaySafeHash(hash).call().await.unwrap();
        let signed_rsh = wallet.sign_hash(&rsh).await.unwrap().as_bytes().to_vec();
        let signature = SignatureWithNonce::abi_encode(&(U256::from(0), signed_rsh));

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
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn deserialization_error_maps_to_invalid_argument() {
        let status = Status::from(GrpcServerError::Deserialization(
            DeserializationError::InvalidAccountId,
        ));

        assert_eq!(status.code(), Code::InvalidArgument);
        assert!(status.metadata().get("error-code").is_some());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn retryable_signature_error_maps_to_unavailable() {
        use xmtp_id::scw_verifier::VerifierError;

        // A missing/unreachable chain verifier is a transient, server-side failure.
        let err = GrpcServerError::Signature(SignatureError::VerifierError(
            VerifierError::NoVerifier("eip155:1".to_string()),
        ));
        assert!(err.is_retryable());

        let status = Status::from(err);
        assert_eq!(status.code(), Code::Unavailable);
        assert!(status.metadata().get("error-code").is_some());
    }
}
