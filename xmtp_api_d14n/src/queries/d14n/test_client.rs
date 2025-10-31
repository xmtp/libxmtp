#![allow(clippy::unwrap_used)]

use xmtp_configuration::PAYER_WRITE_FILTER;
use xmtp_id::scw_verifier::MultiSmartContractSignatureVerifier;
use xmtp_proto::api_client::ToxicProxies;
use xmtp_proto::api_client::ToxicTestClient;
use xmtp_proto::prelude::ApiBuilder;
use xmtp_proto::prelude::XmtpTestClient;
use xmtp_proto::types::AppVersion;

use crate::ReadWriteClientBuilder;
use crate::{protocol::NoCursorStore, ReadWriteClient, XmtpTestClientExt};

use super::*;

impl<R, W> XmtpTestClientExt for D14nClient<ReadWriteClient<R, W>, Arc<dyn CursorStore>>
where
    R: XmtpTestClient<Builder = W::Builder>,
    W: XmtpTestClient,
    W::Builder: Clone,
{
    fn with_cursor_store(store: Arc<dyn CursorStore>) -> <Self as XmtpTestClient>::Builder {
        let mut b = <Self as XmtpTestClient>::create();
        b.store = store;
        b
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<C> ToxicTestClient for D14nClient<C, Arc<dyn CursorStore>>
where
    C: ToxicTestClient,
{
    async fn proxies() -> ToxicProxies {
        <C as ToxicTestClient>::proxies().await
    }
}

impl<Builder1, Builder2> XmtpTestClient
    for D14nClient<ReadWriteClient<Builder1, Builder2>, Arc<dyn CursorStore>>
where
    Builder1: XmtpTestClient<Builder = Builder2::Builder>,
    Builder2: XmtpTestClient,
    Builder2::Builder: Clone,
{
    type Builder = D14nClientBuilder<
        ReadWriteClientBuilder<Builder1::Builder, Builder2::Builder>,
        Arc<dyn CursorStore>,
    >;
    fn create() -> Self::Builder {
        let mut rw = ReadWriteClient::builder();
        rw.read(<Builder1 as XmtpTestClient>::create())
            .write(<Builder2 as XmtpTestClient>::create())
            .filter(PAYER_WRITE_FILTER);
        D14nClientBuilder::new(rw, Arc::new(NoCursorStore))
    }
}

pub struct D14nClientBuilder<C, Store> {
    client: C,
    store: Store,
}

impl<C, Store> D14nClientBuilder<C, Store> {
    pub fn new(client: C, store: Store) -> Self {
        Self { client, store }
    }
}

// TODO: should deprecate these api builders
// it is not a good solution for the variety o clients we have now
// its OK for low-level GRPC clients
impl<BRead, BWrite, Store> ApiBuilder
    for D14nClientBuilder<ReadWriteClientBuilder<BRead, BWrite>, Store>
where
    BRead: ApiBuilder,
    BWrite: ApiBuilder,
{
    type Output = D14nClient<ReadWriteClient<BRead::Output, BWrite::Output>, Store>;
    type Error = <BRead as ApiBuilder>::Error;
    fn set_libxmtp_version(&mut self, _: String) -> Result<(), Self::Error> {
        unimplemented!("no way to set host for a client that needs 2 hosts")
    }

    fn set_app_version(&mut self, _: AppVersion) -> Result<(), Self::Error> {
        unimplemented!("no way to set host for a client that needs 2 hosts")
    }

    fn set_host(&mut self, _host: String) {
        unimplemented!("no way to set host for a client that needs 2 hosts")
    }

    fn set_tls(&mut self, _: bool) {
        unimplemented!("no way to set host for a client that needs 2 hosts")
    }

    fn rate_per_minute(&mut self, _: u32) {
        unimplemented!("no way to set host for a client that needs 2 hosts")
    }

    fn port(&self) -> Result<Option<String>, Self::Error> {
        unimplemented!("no way to set host for a client that needs 2 hosts")
    }

    fn host(&self) -> Option<&str> {
        unimplemented!("no way to set host for a client that needs 2 hosts")
    }

    fn build(self) -> Result<Self::Output, Self::Error> {
        Ok(D14nClient {
            client: <ReadWriteClientBuilder<BRead, BWrite> as ApiBuilder>::build(self.client)
                .unwrap(),
            cursor_store: self.store,
            scw_verifier: Arc::new(MultiSmartContractSignatureVerifier::new_from_env().unwrap()),
        })
    }

    fn set_retry(&mut self, _: xmtp_common::Retry) {
        unimplemented!("no way to set host for a client that needs 2 hosts")
    }
}
