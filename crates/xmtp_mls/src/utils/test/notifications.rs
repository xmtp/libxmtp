//! Clients whose notification requests a scripted peer answers, while every
//! other request reaches the test backend.

pub(crate) use crate::client::notifications::test_support::{Call, Peer, ScriptedApi};
use crate::{context::CoreContext, tester, worker::WorkerConfig};
use std::sync::Arc;

pub(crate) type Context = Arc<
    crate::context::XmtpMlsLocalContext<
        ScriptedApi,
        xmtp_db::DefaultStore,
        crate::utils::TestMlsStorage,
    >,
>;

/// Reuse the normal registered tester, then replace only its notification responses.
pub(crate) async fn client() -> (crate::Client<Context>, Arc<Peer>) {
    tester!(base, disable_workers);
    let peer = Arc::new(Peer::default());
    let api = ScriptedApi::new(
        base.context.api().api_client.raw_for_test().clone(),
        peer.clone(),
    );
    let client = crate::builder::ClientBuilder::from_client(base.client.clone())
        .api_client(api)
        .with_disable_workers(false)
        .worker_config(WorkerConfig::default())
        .with_allow_offline(Some(true))
        .build()
        .await
        .unwrap();
    client.workers.shutdown().await;
    (client, peer)
}
