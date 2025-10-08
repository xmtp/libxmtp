use std::num::NonZeroUsize;
use xmtp_proto::traits::Client;

pub trait MiddlewareBuilder<C>
where
    C: Client + Sync + Send,
{
    type Output;
    type Error;

    /// set the gateway client for node discovery
    fn set_gateway_client(&mut self, gateway_client: C) -> Result<(), Self::Error>;

    /// max timeout allowed for nodes to respond, in milliseconds
    fn set_timeout(&mut self, timeout: NonZeroUsize) -> Result<(), Self::Error>;

    #[allow(async_fn_in_trait)]
    async fn build(self) -> Result<Self::Output, Self::Error>;
}
