use prost::{Message, bytes::Bytes};
use std::borrow::Cow;
use xmtp_proto::{
    api::{BodyError, Endpoint},
    backend_v1,
};

/// Read what the deployment publishes about itself. Unauthenticated, so this
/// is the one call a client makes before it knows whether it needs a
/// credential.
#[derive(Clone, Debug, Default)]
pub struct GetConfiguration(pub backend_v1::GetConfigurationRequest);

/// The one gRPC path the client auth middleware never attaches a credential to.
pub const GET_CONFIGURATION_PATH: &str = "/xmtp.backend.v1.ConfigurationService/GetConfiguration";

impl Endpoint for GetConfiguration {
    type Output = backend_v1::GetConfigurationResponse;
    fn grpc_endpoint(&self) -> Cow<'static, str> {
        GET_CONFIGURATION_PATH.into()
    }
    fn body(&self) -> Result<Bytes, BodyError> {
        Ok(self.0.encode_to_vec().into())
    }
}
