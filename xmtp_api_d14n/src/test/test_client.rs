//! XmtpTestClient impl for real docker/dev backend
use std::sync::Arc;

use crate::{
    D14nClient, D14nClientBuilder, V3Client, V3ClientBuilder,
    protocol::{CursorStore, NoCursorStore},
};
use xmtp_proto::{
    api::Client,
    prelude::{ApiBuilder, XmtpTestClient},
};

/// extends the [`XmtpTestClient`] with a cursor store build method
pub trait XmtpTestClientExt: XmtpTestClient {
    fn with_cursor_store(
        _f: impl Fn() -> <Self as XmtpTestClient>::Builder,
        _store: Arc<dyn CursorStore>,
    ) -> <Self as XmtpTestClient>::Builder {
        unimplemented!(
            "cursor store not available for this type {}",
            std::any::type_name::<Self>()
        )
    }
}

impl<C, Payer> XmtpTestClientExt for D14nClient<C, Payer, Arc<dyn CursorStore>>
where
    C: XmtpTestClient + Client,
    Payer: XmtpTestClient + Client,
    <<C as XmtpTestClient>::Builder as ApiBuilder>::Output: Client,
    <<Payer as XmtpTestClient>::Builder as ApiBuilder>::Output: Client,
    <C as XmtpTestClient>::Builder:
        ApiBuilder<Error = <<Payer as XmtpTestClient>::Builder as ApiBuilder>::Error>,
{
    fn with_cursor_store(
        f: impl Fn() -> <Self as XmtpTestClient>::Builder,
        store: Arc<dyn CursorStore>,
    ) -> <Self as XmtpTestClient>::Builder {
        let mut b = f();
        b.cursor_store(store);
        b
    }
}

impl<C, Payer> XmtpTestClient for D14nClient<C, Payer, Arc<dyn CursorStore>>
where
    C: XmtpTestClient + Client,
    Payer: XmtpTestClient + Client,
    <<C as XmtpTestClient>::Builder as ApiBuilder>::Output: Client,
    <<Payer as XmtpTestClient>::Builder as ApiBuilder>::Output: Client,
    <C as XmtpTestClient>::Builder:
        ApiBuilder<Error = <<Payer as XmtpTestClient>::Builder as ApiBuilder>::Error>,
{
    type Builder = D14nClientBuilder<C::Builder, Payer::Builder, Arc<dyn CursorStore>>;

    fn create_local() -> Self::Builder {
        D14nClientBuilder::new(
            <C as XmtpTestClient>::create_d14n(),
            <Payer as XmtpTestClient>::create_gateway(),
            Arc::new(NoCursorStore),
        )
    }
    /*
    fn create_with_store(store: Arc<dyn CursorStore>) -> Self::Builder {
        D14nClientBuilder::new(
            <C as XmtpTestClient>::create_d14n(),
            <Payer as XmtpTestClient>::create_gateway(),
            store,
        )
    }
    */

    fn create_gateway() -> Self::Builder {
        D14nClientBuilder::new(
            <C as XmtpTestClient>::create_gateway(),
            <Payer as XmtpTestClient>::create_gateway(),
            Arc::new(NoCursorStore),
        )
    }
    fn create_d14n() -> Self::Builder {
        D14nClientBuilder::new(
            <C as XmtpTestClient>::create_d14n(),
            <Payer as XmtpTestClient>::create_gateway(),
            Arc::new(NoCursorStore),
        )
    }
}

impl<C> XmtpTestClient for V3Client<C, Arc<dyn CursorStore>>
where
    C: XmtpTestClient + Client,
    <<C as XmtpTestClient>::Builder as ApiBuilder>::Output: Client,
{
    type Builder = V3ClientBuilder<C::Builder, Arc<dyn CursorStore>>;
    fn create_local() -> Self::Builder {
        V3ClientBuilder::new(
            <C as XmtpTestClient>::create_local(),
            Arc::new(NoCursorStore),
        )
    }

    fn create_dev() -> Self::Builder {
        V3ClientBuilder::new(<C as XmtpTestClient>::create_dev(), Arc::new(NoCursorStore))
    }
    fn create_gateway() -> Self::Builder {
        V3ClientBuilder::new(
            <C as XmtpTestClient>::create_gateway(),
            Arc::new(NoCursorStore),
        )
    }
    fn create_d14n() -> Self::Builder {
        V3ClientBuilder::new(
            <C as XmtpTestClient>::create_d14n(),
            Arc::new(NoCursorStore),
        )
    }
}

impl<C> XmtpTestClientExt for V3Client<C, Arc<dyn CursorStore>>
where
    C: XmtpTestClient + Client,
    <<C as XmtpTestClient>::Builder as ApiBuilder>::Output: Client,
{
    fn with_cursor_store(
        f: impl Fn() -> <Self as XmtpTestClient>::Builder,
        store: Arc<dyn CursorStore>,
    ) -> <Self as XmtpTestClient>::Builder {
        let mut b = f();
        b.cursor_store(store);
        b
    }
}
