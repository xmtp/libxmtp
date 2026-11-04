//! A client's deferred configuration check runs before its subscription
//! factory opens anything.

use crate::context::CoreContext;
use crate::server_configuration::test_support::scripted_api::*;
use crate::subscriptions::incoming::{IncomingRuntime, SubscriptionFactory, SubscriptionFuture};
use crate::utils::test::preflight::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::oneshot;
use xmtp_db::prelude::*;
use xmtp_proto::types::{IncomingBatchLimits, TopicCursor};

struct CountedFactory(Arc<AtomicUsize>);
impl SubscriptionFactory for CountedFactory {
    fn open(&self, _: TopicCursor, _: IncomingBatchLimits) -> SubscriptionFuture {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(xmtp_proto::types::IncomingSubscription::new(
                Box::pin(futures::stream::pending()),
                |_| {},
            ))
        })
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn deferred_preflight_precedes_even_synchronous_factory_effects() {
    let (client, script) = fixture().await;
    let count = Arc::new(AtomicUsize::new(0));
    let runtime = IncomingRuntime::new(
        Default::default(),
        Some(Arc::new(CountedFactory(count.clone()))),
    )
    .with_preflight(client.context.api().api_client.clone());
    let (send, receive) = oneshot::channel();
    *script.pause.lock() = Some(receive);
    let mut open = runtime.factory_for_test().unwrap().open(
        Default::default(),
        IncomingBatchLimits {
            max_rows: 1,
            max_bytes: 1024,
        },
    );
    assert_eq!(
        count.load(Ordering::SeqCst),
        0,
        "creating the future must not open the factory"
    );
    assert!(futures::poll!(&mut open).is_pending());
    assert_eq!(count.load(Ordering::SeqCst), 0);
    send.send(()).unwrap();
    open.await?;
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(
        script.db.db().server_configuration()?.unwrap().backend_url,
        NEW
    );
    client.close().await?;
}
