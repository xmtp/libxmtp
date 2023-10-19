use tonic::{Request, Response, Status};

use xmtp_proto::xmtp::mls_validation::v1::{
    validation_api_server::ValidationApi, ValidateBasicIdentitiesRequest,
    ValidateBasicIdentitiesResponse, ValidateGroupMessagesRequest, ValidateGroupMessagesResponse,
    ValidateKeyPackagesRequest, ValidateKeyPackagesResponse,
};

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

    async fn validate_basic_identities(
        &self,
        request: Request<ValidateBasicIdentitiesRequest>,
    ) -> Result<Response<ValidateBasicIdentitiesResponse>, Status> {
        Err(Status::unimplemented("not implemented"))
    }

    async fn validate_group_messages(
        &self,
        request: Request<ValidateGroupMessagesRequest>,
    ) -> Result<Response<ValidateGroupMessagesResponse>, Status> {
        Err(Status::unimplemented("not implemented"))
    }
}
