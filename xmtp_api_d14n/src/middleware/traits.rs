use xmtp_api_grpc::GrpcClient;
use xmtp_common::time::Duration;

/* Middleware trait */

pub trait MiddlewareBuilder {
    type Output;
    type Error;

    /// set the gateway client for node discovery
    fn set_gateway_client(&mut self, gateway_client: GrpcClient) -> Result<(), Self::Error>;

    /// max timeout allowed for nodes to respond
    fn set_timeout(&mut self, timeout: Duration) -> Result<(), Self::Error>;

    fn build(self) -> Result<Self::Output, Self::Error>;
}
