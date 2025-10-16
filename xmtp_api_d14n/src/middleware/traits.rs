use xmtp_api_grpc::ClientBuilder;
use xmtp_common::time::Duration;

/* Middleware trait */

pub trait MiddlewareBuilder {
    type Output;
    type Error;

    /// set the gateway builder for node discovery
    fn set_gateway_builder(&mut self, gateway_builder: ClientBuilder) -> Result<(), Self::Error>;

    /// max timeout allowed for nodes to respond
    fn set_timeout(&mut self, timeout: Duration) -> Result<(), Self::Error>;

    fn build(self) -> Result<Self::Output, Self::Error>;
}
