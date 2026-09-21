//! Read-only deployment settings, served without a credential.
//!
//! The response is built once from the validated configuration and never
//! changes while the process runs, so this handler reads no state and touches
//! no database.

#[cfg(test)]
mod tests;

use crate::{Backend, api};
use tonic::{Request, Response, Status};

#[tonic::async_trait]
impl api::configuration_service_server::ConfigurationService for Backend {
    #[xmtp_common::rpc_span]
    // implements: CONF-012
    async fn get_configuration(
        &self,
        _request: Request<api::GetConfigurationRequest>,
    ) -> Result<Response<api::GetConfigurationResponse>, Status> {
        Ok(Response::new((*self.configuration).clone()))
    }
}
