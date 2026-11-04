//! A public reader reports a client's failed configuration check, and a
//! replacement reader checks again.

use crate::utils::test::preflight::*;
use xmtp_common::RetryableError;
use xmtp_mls_engine::groups::context::XmtpSharedContext;
use xmtp_mls_server_config::server_configuration::ServerConfigError;
use xmtp_proto::api::ApiClientError;

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
            assert!(matches!(cause(&error), ServerConfigError::Unavailable(_)));
            assert_eq!(error.is_retryable(), retryable);
            assert!(
                client
                    .context
                    .incoming_runtime()
                    .rejected_requests_for_test()
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
