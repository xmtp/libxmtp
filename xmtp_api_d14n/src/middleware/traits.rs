use xmtp_api_grpc::ClientBuilder;
use xmtp_common::time::Duration;

/* Middleware trait */

pub trait MiddlewareBuilder {
    type Output;
    type Error;

    /// Set the gateway builder for node discovery.
    fn set_gateway_builder(&mut self, gateway_builder: ClientBuilder) -> Result<(), Self::Error>;

    /// Set the timeout for node discovery.
    fn set_timeout(&mut self, timeout: Duration) -> Result<(), Self::Error>;

    /// Build the middleware.
    fn build(self) -> Result<Self::Output, Self::Error>;
}
