//! XmtpTestClient impl for real docker/dev backend
use xmtp_proto::{
    api::Client,
    prelude::{ApiBuilder, XmtpTestClient},
};

use crate::{D14nClient, D14nClientBuilder, V3Client, V3ClientBuilder};

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
        D14nClientBuilder::new(
            <C as XmtpTestClient>::create_d14n(),
            <Payer as XmtpTestClient>::create_payer(),
        )
    }
    fn create_dev() -> Self::Builder {
        // TODO: Staging
        panic!("no urls for d14n dev yet");
    }
    fn create_payer() -> Self::Builder {
        D14nClientBuilder::new(
            <C as XmtpTestClient>::create_payer(),
            <Payer as XmtpTestClient>::create_payer(),
        )
    }
    fn create_d14n() -> Self::Builder {
        D14nClientBuilder::new(
            <C as XmtpTestClient>::create_d14n(),
            <Payer as XmtpTestClient>::create_payer(),
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
        V3ClientBuilder::new(<C as XmtpTestClient>::create_local())
    }
    fn create_dev() -> Self::Builder {
        V3ClientBuilder::new(<C as XmtpTestClient>::create_dev())
    }
    fn create_payer() -> Self::Builder {
        V3ClientBuilder::new(<C as XmtpTestClient>::create_payer())
    }
    fn create_d14n() -> Self::Builder {
        V3ClientBuilder::new(<C as XmtpTestClient>::create_d14n())
    }
}
