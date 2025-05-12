//! Compatibility layer for d14n and previous xmtp_api crate
mod identity;
mod mls;
mod streams;

use xmtp_proto::prelude::ApiBuilder;

#[derive(Clone)]
pub struct D14nClient<C, P> {
    message_client: C,
    payer_client: P,
}

impl<C, P> D14nClient<C, P> {
    pub fn new(message_client: C, payer_client: P) -> Self {
        Self {
            message_client,
            payer_client,
        }
    }
}

pub struct D14nClientBuilder<Builder1, Builder2> {
    message_client: Builder1,
    payer_client: Builder2,
}

impl<Builder1, Builder2> D14nClientBuilder<Builder1, Builder2> {
    pub fn new(message_client: Builder1, payer_client: Builder2) -> Self {
        Self {
            message_client,
            payer_client,
        }
    }
}

impl<Builder1, Builder2> ApiBuilder for D14nClientBuilder<Builder1, Builder2>
where
    Builder1: ApiBuilder<Error = <Builder2 as ApiBuilder>::Error>,
    Builder2: ApiBuilder,
    <Builder1 as ApiBuilder>::Output: xmtp_proto::traits::Client,
    <Builder2 as ApiBuilder>::Output: xmtp_proto::traits::Client,
{
    type Output = D14nClient<<Builder1 as ApiBuilder>::Output, <Builder2 as ApiBuilder>::Output>;

    type Error = <Builder1 as ApiBuilder>::Error;

    fn set_libxmtp_version(&mut self, version: String) -> Result<(), Self::Error> {
        <Builder1 as ApiBuilder>::set_libxmtp_version(&mut self.message_client, version.clone())?;
        <Builder2 as ApiBuilder>::set_libxmtp_version(&mut self.payer_client, version)
    }

    fn set_app_version(&mut self, version: String) -> Result<(), Self::Error> {
        <Builder1 as ApiBuilder>::set_app_version(&mut self.message_client, version.clone())?;
        <Builder2 as ApiBuilder>::set_app_version(&mut self.payer_client, version)
    }

    // TODO: Add a builder method for the payer host
    fn set_host(&mut self, host: String) {
        <Builder1 as ApiBuilder>::set_host(&mut self.message_client, host.clone());
        <Builder2 as ApiBuilder>::set_host(&mut self.payer_client, host)
    }

    fn set_tls(&mut self, tls: bool) {
        <Builder1 as ApiBuilder>::set_tls(&mut self.message_client, tls);
        <Builder2 as ApiBuilder>::set_tls(&mut self.payer_client, tls)
    }

    async fn build(self) -> Result<Self::Output, Self::Error> {
        Ok(D14nClient::new(
            <Builder1 as ApiBuilder>::build(self.message_client).await?,
            <Builder2 as ApiBuilder>::build(self.payer_client).await?,
        ))
    }

    fn rate_per_minute(&mut self, limit: u32) {
        <Builder1 as ApiBuilder>::rate_per_minute(&mut self.message_client, limit);
        <Builder2 as ApiBuilder>::rate_per_minute(&mut self.payer_client, limit)
    }
}
