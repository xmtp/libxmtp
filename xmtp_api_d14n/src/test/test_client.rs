//! XmtpTestClient impl for real docker/dev backend
use crate::{D14nClient, D14nClientBuilder, V3Client, V3ClientBuilder, protocol::NoCursorStore};
use xmtp_proto::{
    api::Client,
    prelude::{ApiBuilder, XmtpTestClient},
};

impl<C> XmtpTestClient for D14nClient<C, NoCursorStore>
where
    C: XmtpTestClient + Client,
    <<C as XmtpTestClient>::Builder as ApiBuilder>::Output: Client,
{
    type Builder = D14nClientBuilder<C::Builder>;

    fn create() -> Self::Builder {
        D14nClient::new(<C as XmtpTestClient>::create(), NoCursorStore)
    }
}

impl<C> XmtpTestClient for V3Client<C, NoCursorStore>
where
    C: XmtpTestClient + Client,
    <<C as XmtpTestClient>::Builder as ApiBuilder>::Output: Client,
{
    type Builder = V3ClientBuilder<C::Builder, NoCursorStore>;
    fn create() -> Self::Builder {
        V3ClientBuilder::new(<C as XmtpTestClient>::create(), NoCursorStore)
    }
}
