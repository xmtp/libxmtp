use xmtp_api_grpc::ClientBuilder;
use xmtp_common::time::Duration;
use xmtp_proto::prelude::ApiBuilder;

/* Middleware trait */

pub trait MiddlewareBuilder: ApiBuilder {
    /// Set the gateway builder for node discovery.
    fn set_gateway_builder(&mut self, gateway_builder: ClientBuilder) -> Result<(), Self::Error>;
    /// Set the default builder for xmtpd nodes
    fn set_node_client_builder(&mut self, node_builder: ClientBuilder) -> Result<(), Self::Error>;

    /// Set the timeout for node discovery.
    fn set_timeout(&mut self, timeout: Duration) -> Result<(), Self::Error>;
}
