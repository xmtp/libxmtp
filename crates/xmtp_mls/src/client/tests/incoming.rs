//! The incoming coordinator as a client sets it up: the transport a client
//! build selects, a query-only API client, and the context released by
//! closing the client.

use crate::prelude::*;
use crate::subscriptions::incoming::{
    IncomingCoordinator, IncomingProcessing, IncomingRegistration, IncomingScope,
};
use crate::tester;
use std::sync::Arc;
use xmtp_db::prelude::*;
use xmtp_proto::types::Topic;

#[xmtp_common::test(unwrap_try = true)]
async fn client_setup_selects_the_platform_transport_without_starting_it() {
    use xmtp_proto::api::HasStats;
    use xmtp_proto::api_client::{ApiBuilder, XmtpTestClient};

    tester!(seed, disable_workers);
    let api = Arc::new(crate::utils::DefaultTestClientCreator::create().build()?);
    #[cfg(not(target_arch = "wasm32"))]
    let _wires_before = crate::subscriptions::router_callbacks::shared_transport_count();
    // Registration uses an explicit barrier. Rebuild the registered identity
    // with a new API client to observe construction before its first interest.
    let alix = crate::builder::ClientBuilder::from_client(seed.client.clone())
        .api_client_with_streams(api)
        .with_disable_workers(true)
        .with_allow_offline(Some(true))
        .build()
        .await?;
    let stats = alix.context.api().api_client.mls_stats();
    let coordinator = IncomingCoordinator::for_context(&alix.context);
    let empty = coordinator.acquire(IncomingScope::Topics(vec![]));
    xmtp_common::wait_for_eq(
        || async { empty.snapshot().processing },
        IncomingProcessing::Complete,
    )
    .await?;
    assert_eq!(stats.subscribe.get_count(), 0);
    assert_eq!(stats.subscribe_static.get_count(), 0);
    xmtp_common::if_native! { @
        assert_eq!(crate::subscriptions::router_callbacks::shared_transport_count(), _wires_before);
    }

    let topic = Topic::new_welcome_message(alix.context.installation_id());
    let first = coordinator.acquire(IncomingScope::Topics(vec![topic.clone()]));
    let second =
        IncomingCoordinator::for_context(&alix.context).acquire(IncomingScope::Topics(vec![topic]));
    for lease in [&first, &second] {
        xmtp_common::wait_for_eq(
            || async {
                lease
                    .snapshot()
                    .topics
                    .first()
                    .map(|topic| topic.registration)
            },
            Some(IncomingRegistration::Active),
        )
        .await?;
    }
    xmtp_common::if_native! { @
        assert_eq!(stats.subscribe.get_count(), 1);
        assert_eq!(stats.subscribe_static.get_count(), 0);
        assert_eq!(crate::subscriptions::router_callbacks::shared_transport_count(), _wires_before + 1);
    }
    xmtp_common::if_wasm! { @
        assert_eq!(stats.subscribe.get_count(), 0);
        assert_eq!(stats.subscribe_static.get_count(), 1);
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn releasing_readers_allows_recreation_and_releases_the_context() {
    tester!(alix, disable_workers);
    let weak_context = Arc::downgrade(&alix.context);
    let topic = Topic::new_welcome_message(alix.context.installation_id());
    for _ in 0..2 {
        let coordinator = IncomingCoordinator::for_context(&alix.context);
        let lease = coordinator.acquire(IncomingScope::Topics(vec![topic.clone()]));
        xmtp_common::wait_for_eq(
            || async {
                lease
                    .snapshot()
                    .topics
                    .first()
                    .map(|topic| topic.registration)
            },
            Some(IncomingRegistration::Active),
        )
        .await?;
        drop(lease);
        drop(coordinator);
        xmtp_common::wait_for_eq(
            || async { alix.context.incoming_runtime().coordinator.lock().is_none() },
            true,
        )
        .await?;
    }
    alix.close().await?;
    drop(alix);
    xmtp_common::wait_for_eq(|| async { weak_context.strong_count() }, 0).await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn replacing_the_api_with_query_only_setup_receives_and_processes_messages() {
    use crate::groups::send_message_opts::SendMessageOpts;
    use xmtp_proto::api::HasStats;
    use xmtp_proto::api_client::{ApiBuilder, XmtpTestClient};

    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = bo.create_group(None, None)?;
    group.add_members(&[alix.inbox_id()]).await?;
    alix.sync_welcomes().await?;
    let message_id = group
        .send_message(b"query-only receipt", SendMessageOpts::default())
        .await?;
    let api = Arc::new(crate::utils::DefaultTestClientCreator::create().build()?);
    let stats = api.mls_stats();
    let client = crate::builder::ClientBuilder::from_client(alix.client.clone())
        .api_client(api)
        .with_disable_workers(true)
        .with_allow_offline(Some(true))
        .build()
        .await?;
    stats.clear();
    let lease = IncomingCoordinator::for_context(&client.context)
        .acquire(IncomingScope::Groups(vec![group.group_id]));
    xmtp_common::wait_for_eq(
        || async { lease.snapshot().processing },
        IncomingProcessing::Complete,
    )
    .await?;
    let message = client.context.db().get_group_message(&message_id)?.unwrap();
    assert_eq!(message.decrypted_message_bytes, b"query-only receipt");
    assert!(stats.query_newest.get_count() > 0);
    assert!(stats.query.get_count() > 0);
    assert_eq!(stats.subscribe.get_count(), 0);
    assert_eq!(stats.subscribe_static.get_count(), 0);
}
