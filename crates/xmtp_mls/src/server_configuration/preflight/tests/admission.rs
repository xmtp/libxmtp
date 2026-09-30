use super::*;
use crate::subscriptions::incoming::{IncomingRuntime, SubscriptionFactory, SubscriptionFuture};
use std::sync::atomic::{AtomicUsize, Ordering};
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
    let mut open = runtime.factory.as_ref().unwrap().open(
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

#[xmtp_common::test(unwrap_try = true)]
async fn deferred_preflight_cancelled_waiter_does_not_cancel_leader() {
    let (client, script) = fixture().await;
    let (send, receive) = oneshot::channel();
    *script.pause.lock() = Some(receive);
    let mut leader = Box::pin(query(&client));
    assert!(futures::poll!(&mut leader).is_pending());
    let mut waiter = Box::pin(query(&client));
    assert!(futures::poll!(&mut waiter).is_pending());
    drop(waiter);
    send.send(()).unwrap();
    leader.await?;
    assert_eq!(*script.calls.lock(), vec!["configuration", "query"]);
    client.close().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn deferred_preflight_successful_refresh_discharges_pending_without_replacing_snapshot() {
    let (client, script) = fixture().await;
    let (send, receive) = oneshot::channel();
    *script.pause.lock() = Some(receive);
    let refresh = client.refresh_server_configuration();
    let target = query(&client);
    futures::pin_mut!(refresh, target);
    assert!(futures::poll!(&mut refresh).is_pending());
    assert!(futures::poll!(&mut target).is_pending());
    send.send(()).unwrap();
    let (refresh, target) = futures::join!(refresh, target);
    refresh?;
    target?;
    assert_eq!(*script.calls.lock(), vec!["configuration", "query"]);
    client.close().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn deferred_preflight_public_reader_reports_first_failure_and_replacement_can_retry() {
    use crate::subscriptions::{
        local_delivery::{DeliveryScope, LocalDeliveryFilter},
        message_reader::MessageReader,
    };
    for streams in [false, true] {
        for retryable in [false, true] {
            let (client, script) = fixture_options(streams, false).await;
            let group = client.create_group(None, None)?;
            // Keep every concurrent internal request failing until the public reader
            // reports the first failure. A replacement operation is repaired below.
            for _ in 0..32 {
                script
                    .responses
                    .lock()
                    .push_back(Err(ApiClientError::client(
                        xmtp_api_grpc::error::GrpcError::Status(tonic::Status::new(
                            if retryable {
                                tonic::Code::Unavailable
                            } else {
                                tonic::Code::InvalidArgument
                            },
                            "scripted configuration failure",
                        )),
                    )));
            }
            let scope = DeliveryScope::Groups(vec![group.group_id]);
            let mut reader = MessageReader::new(
                client.context.clone(),
                scope.clone(),
                LocalDeliveryFilter::default(),
                None,
            )?;
            let error =
                xmtp_common::time::timeout(xmtp_common::time::Duration::from_secs(5), async {
                    loop {
                        match reader.next_delivery().await {
                            Ok(Some(item)) => item.acknowledgement.acknowledge().unwrap(),
                            Ok(None) => panic!("reader hid its configuration failure"),
                            Err(error) => break error,
                        }
                    }
                })
                .await?;
            assert!(matches!(
                cause(&error),
                ClientError::ConfigurationUnavailable(_)
            ));
            assert_eq!(error.is_retryable(), retryable);
            assert!(
                client
                    .context
                    .incoming_runtime()
                    .rejected_requests
                    .lock()
                    .is_empty()
            );
            script.responses.lock().clear();
            let mut replacement = MessageReader::new(
                client.context.clone(),
                scope,
                LocalDeliveryFilter::default(),
                None,
            )?;
            let mut next = Box::pin(replacement.next_delivery());
            let mut seen = Box::pin(async {
                xmtp_common::wait_for_ge(
                    || async {
                        script
                            .calls
                            .lock()
                            .iter()
                            .filter(|&&call| call != "configuration")
                            .count()
                    },
                    1,
                )
                .await
                .unwrap();
            });
            tokio::select! {
                error = &mut next => { assert!(xmtp_api::preflight::failure(&error.err().expect("no stale preflight failure")).is_none()); },
                _ = &mut seen => {},
            }
            drop(next);
            replacement.close();
            client.close().await?;
        }
    }
}
