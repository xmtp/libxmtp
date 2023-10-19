use tonic::{Request, Response, Status};

use openmls::credentials::BasicCredential;
use openmls::prelude::TlsDeserializeTrait;
use xmtp_proto::xmtp::mls_validation::v1::{
    validate_identities_response::ValidationResponse, validation_api_server::ValidationApi,
    ValidateGroupMessagesRequest, ValidateGroupMessagesResponse, ValidateIdentitiesRequest,
    ValidateIdentitiesResponse, ValidateKeyPackagesRequest, ValidateKeyPackagesResponse,
};

use crate::validation_helpers::{identity_to_wallet_address, pub_key_to_installation_id};

#[derive(Debug, Default)]
pub struct ValidationServer {}

#[tonic::async_trait]
impl ValidationApi for ValidationServer {
    async fn validate_key_packages(
        &self,
        request: Request<ValidateKeyPackagesRequest>,
    ) -> Result<Response<ValidateKeyPackagesResponse>, Status> {
        Err(Status::unimplemented("not implemented"))
    }

    async fn validate_identities(
        &self,
        request: Request<ValidateIdentitiesRequest>,
    ) -> Result<Response<ValidateIdentitiesResponse>, Status> {
        let identities = request.into_inner().credentials;
        let mut out: Vec<ValidationResponse> = vec![];

        for identity in identities {
            let pub_key_bytes = identity.signing_public_key_bytes.as_slice();
            let wallet_address =
                identity_to_wallet_address(identity.identity_bytes.as_slice(), pub_key_bytes);

            if wallet_address.is_err() {
                out.push(ValidationResponse {
                    installation_id: "".to_string(),
                    error_message: wallet_address
                        .err()
                        .ok_or("could not derive wallet address".to_string())
                        .unwrap(),
                    wallet_address: "".to_string(),
                    is_ok: false,
                });

                continue;
            }

            out.push(ValidationResponse {
                installation_id: pub_key_to_installation_id(pub_key_bytes),
                error_message: "".to_string(),
                wallet_address: wallet_address.unwrap(),
                is_ok: true,
            })
        }

        Ok(Response::new(ValidateIdentitiesResponse { responses: out }))
    }

    async fn validate_group_messages(
        &self,
        request: Request<ValidateGroupMessagesRequest>,
    ) -> Result<Response<ValidateGroupMessagesResponse>, Status> {
        Err(Status::unimplemented("not implemented"))
    }
}
