#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use xmtp_common::MaybeSend;
use xmtp_common::MaybeSync;
use xmtp_configuration::PAYER_WRITE_FILTER;
use xmtp_id::scw_verifier::MultiSmartContractSignatureVerifier;
use xmtp_proto::api_client::ToxicProxies;
use xmtp_proto::api_client::ToxicTestClient;
use xmtp_proto::prelude::ApiBuilder;
use xmtp_proto::prelude::XmtpTestClient;

use crate::ReadWriteClientBuilder;
use crate::protocol::CursorStore;
use crate::{ReadWriteClient, XmtpTestClientExt, protocol::NoCursorStore};

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

#[xmtp_common::async_trait]
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

impl<BRead, BWrite, Store> ApiBuilder
    for D14nClientBuilder<ReadWriteClientBuilder<BRead, BWrite>, Store>
where
    BRead: ApiBuilder,
    BWrite: ApiBuilder,
    Store: MaybeSend + MaybeSync,
{
    type Output = D14nClient<ReadWriteClient<BRead::Output, BWrite::Output>, Store>;
    type Error = <BRead as ApiBuilder>::Error;

    fn build(self) -> Result<Self::Output, Self::Error> {
        Ok(D14nClient {
            client: <ReadWriteClientBuilder<BRead, BWrite> as ApiBuilder>::build(self.client)
                .unwrap(),
            cursor_store: self.store,
            scw_verifier: Arc::new(MultiSmartContractSignatureVerifier::new_from_env().unwrap()),
        })
    }
}
