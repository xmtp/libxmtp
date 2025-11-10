use std::sync::Arc;

use xmtp_proto::{
    api_client::{ToxicProxies, ToxicTestClient},
    prelude::XmtpTestClient,
};

use crate::{
    XmtpTestClientExt,
    protocol::{CursorStore, NoCursorStore},
};

use super::*;

impl<C> XmtpTestClient for V3Client<C, Arc<dyn CursorStore>>
where
    C: XmtpTestClient,
{
    type Builder = V3ClientBuilder<C::Builder, Arc<dyn CursorStore>>;
    fn create() -> Self::Builder {
        V3ClientBuilder::new(<C as XmtpTestClient>::create(), Arc::new(NoCursorStore))
    }
}

impl<C> XmtpTestClientExt for V3Client<C, Arc<dyn CursorStore>>
where
    C: XmtpTestClient,
{
    fn with_cursor_store(store: Arc<dyn CursorStore>) -> <Self as XmtpTestClient>::Builder {
        let mut b = <Self as XmtpTestClient>::create();
        b.cursor_store(store);
        b
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C> ToxicTestClient for V3Client<C, Arc<dyn CursorStore>>
where
    C: ToxicTestClient,
{
    async fn proxies() -> ToxicProxies {
        <C as ToxicTestClient>::proxies().await
    }
}
