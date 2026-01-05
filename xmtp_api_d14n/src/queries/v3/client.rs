use xmtp_common::{MaybeSend, MaybeSync};
use xmtp_proto::api::IsConnectedCheck;
use xmtp_proto::prelude::ApiBuilder;

#[derive(Clone)]
pub struct V3Client<C, Store> {
    pub(super) client: C,
    pub(super) cursor_store: Store,
}

impl<C, Store> V3Client<C, Store> {
    pub fn new(client: C, cursor_store: Store) -> Self {
        Self {
            client,
            cursor_store,
        }
    }

    pub fn client_mut(&mut self) -> &mut C {
        &mut self.client
    }
}

#[cfg_attr(any(test, feature = "test-utils"), derive(Clone))]
pub struct V3ClientBuilder<Builder, Store> {
    client: Builder,
    store: Store,
}

impl<Builder, Store> V3ClientBuilder<Builder, Store> {
    pub fn new(client: Builder, store: Store) -> Self {
        Self { client, store }
    }

    pub fn cursor_store(&mut self, store: Store) -> &mut Self {
        self.store = store;
        self
    }
}

impl<Builder, Store> ApiBuilder for V3ClientBuilder<Builder, Store>
where
    Builder: ApiBuilder,
    Store: MaybeSend + MaybeSync,
{
    type Output = V3Client<<Builder as ApiBuilder>::Output, Store>;

    type Error = <Builder as ApiBuilder>::Error;
    fn build(self) -> Result<Self::Output, Self::Error> {
        Ok(V3Client {
            client: <Builder as ApiBuilder>::build(self.client)?,
            cursor_store: self.store,
        })
    }
}

#[xmtp_common::async_trait]
impl<C, Store> IsConnectedCheck for V3Client<C, Store>
where
    C: IsConnectedCheck,
    Store: MaybeSend + MaybeSync,
{
    async fn is_connected(&self) -> bool {
        self.client.is_connected().await
    }
}
