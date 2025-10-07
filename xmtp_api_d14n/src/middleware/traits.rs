use xmtp_common::time::Duration;
use xmtp_proto::traits::Client;

pub trait MiddlewareBuilder<C>
where
    C: Client + Sync + Send,
{
    type Output;
    type Error;

    /// set the gateway client
    fn set_gateway_client(&mut self, gateway_client: C) -> Result<(), Self::Error>;

    /// set the timeout
    fn set_timeout(&mut self, timeout: Duration) -> Result<(), Self::Error>;

    #[allow(async_fn_in_trait)]
    /// Build the api client
    async fn build(self) -> Result<Self::Output, Self::Error>;
}
