//! XmtpTestClient impl for real docker/dev backend
use crate::{D14nClient, D14nClientBuilder, V3Client, V3ClientBuilder};
use xmtp_proto::{
    api::Client,
    prelude::{ApiBuilder, XmtpTestClient},
};

impl<C, Payer> XmtpTestClient for D14nClient<C, Payer>
where
    C: XmtpTestClient + Client,
    Payer: XmtpTestClient + Client,
    <<C as XmtpTestClient>::Builder as ApiBuilder>::Output: Client,
    <<Payer as XmtpTestClient>::Builder as ApiBuilder>::Output: Client,
    <C as XmtpTestClient>::Builder:
        ApiBuilder<Error = <<Payer as XmtpTestClient>::Builder as ApiBuilder>::Error>,
{
    type Builder = D14nClientBuilder<C::Builder, Payer::Builder>;

    fn create_local() -> Self::Builder {
        D14nClientBuilder::new_stateless(
            <C as XmtpTestClient>::create_d14n(),
            <Payer as XmtpTestClient>::create_gateway(),
        )
    }
    fn create_dev() -> Self::Builder {
        // TODO: Staging
        panic!("no urls for d14n dev yet");
    }
    fn create_gateway() -> Self::Builder {
        D14nClientBuilder::new_stateless(
            <C as XmtpTestClient>::create_gateway(),
            <Payer as XmtpTestClient>::create_gateway(),
        )
    }
    fn create_d14n() -> Self::Builder {
        D14nClientBuilder::new_stateless(
            <C as XmtpTestClient>::create_d14n(),
            <Payer as XmtpTestClient>::create_gateway(),
        )
    }
}

impl<C> XmtpTestClient for V3Client<C>
where
    C: XmtpTestClient + Client,
    <<C as XmtpTestClient>::Builder as ApiBuilder>::Output: Client,
{
    type Builder = V3ClientBuilder<C::Builder>;
    fn create_local() -> Self::Builder {
        V3ClientBuilder::new_stateless(<C as XmtpTestClient>::create_local())
    }
    fn create_dev() -> Self::Builder {
        V3ClientBuilder::new_stateless(<C as XmtpTestClient>::create_dev())
    }
    fn create_gateway() -> Self::Builder {
        V3ClientBuilder::new_stateless(<C as XmtpTestClient>::create_gateway())
    }
    fn create_d14n() -> Self::Builder {
        V3ClientBuilder::new_stateless(<C as XmtpTestClient>::create_d14n())
    }
}
