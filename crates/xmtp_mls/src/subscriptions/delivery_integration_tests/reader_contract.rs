//! Fixed reader selection and shared network interest proofs.

use super::*;
use crate::subscriptions::incoming::{IncomingRegistration, SubscriptionFuture};
use std::sync::Arc;
use tokio::sync::Notify;
use xmtp_db::{consent_record::ConsentState, delivery::QueryDelivery, prelude::*};
use xmtp_proto::types::Topic;

// verifies: PROC-046
#[xmtp_common::test(unwrap_try = true)]
async fn reader_filter_does_not_authorize_network() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let denied = alix.create_group(None, None)?;
    let allowed = alix.create_group(None, None)?;
    denied.invite(&bo).await?;
    allowed.invite(&bo).await?;
    bo.sync_welcomes().await?;
    let bo_denied = bo.group(&denied.group_id)?;
    bo_denied.update_consent_state(ConsentState::Allowed)?;
    bo.group(&allowed.group_id)?
        .update_consent_state(ConsentState::Allowed)?;
    let mut reader = MessageReader::new(
        bo.context.clone(),
        DeliveryScope::All,
        LocalDeliveryFilter {
            consent_states: Some(vec![ConsentState::Allowed]),
            ..Default::default()
        },
        None,
    )?;
    let control = reader.control();
    let denied_topic = Topic::new_group_message(denied.group_id);
    xmtp_common::wait_for_eq(
        || async {
            control.catch_up_snapshot().topics.iter().any(|topic| {
                topic.topic == denied_topic && topic.registration == IncomingRegistration::Active
            })
        },
        true,
    )
    .await?;
    bo_denied.update_consent_state(ConsentState::Denied)?;
    let generation = control.catch_up_snapshot().scope_generation;
    // Reconcile the same scope after consent changes, without changing the filter.
    control.update_scope(DeliveryScope::All);
    xmtp_common::wait_for_eq(
        || async {
            let status = control.catch_up_snapshot();
            status.scope_generation > generation && !status.discovery_pending
        },
        true,
    )
    .await?;
    let id = denied
        .send_message(b"protocol still receives denied", Default::default())
        .await?;
    let db = bo.context.db();
    xmtp_common::wait_for_eq(
        || async { db.get_group_message(&id).unwrap().is_some() },
        true,
    )
    .await?;
    assert_eq!(
        db.get_group_message(&id)?.unwrap().decrypted_message_bytes,
        b"protocol still receives denied"
    );
    assert!(control.catch_up_snapshot().topics.iter().any(|topic| {
        topic.topic == denied_topic && topic.registration == IncomingRegistration::Active
    }));
    allowed.send_msg(b"allowed handoff").await;
    let delivered = next_application(&mut reader).await?;
    assert_eq!(delivered.decrypted_message_bytes, b"allowed handoff");
    assert_ne!(delivered.id, id);
    reader.close();
}

// verifies: PROC-048
#[xmtp_common::test(unwrap_try = true)]
async fn local_delivery_precedes_registration_ack() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.send_msg(b"already stored").await;
    let mut builder =
        crate::builder::ClientBuilder::from_client(alix.client.clone()).with_disable_workers(true);
    let factory = builder
        .incoming_factory
        .take()
        .expect("live subscription factory");
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    builder.incoming_factory = Some(Arc::new({
        let entered = entered.clone();
        let release = release.clone();
        move |cursors, limits| -> SubscriptionFuture {
            let factory = factory.clone();
            let entered = entered.clone();
            let release = release.clone();
            Box::pin(async move {
                entered.notify_one();
                release.notified().await;
                factory.open(cursors, limits).await
            })
        }
    }));
    let client = builder.build().await?;
    let mut reader = MessageReader::new(
        client.context.clone(),
        DeliveryScope::Groups(vec![group.group_id]),
        LocalDeliveryFilter::default(),
        None,
    )?;
    xmtp_common::time::timeout(WAIT, entered.notified()).await?;
    let control = reader.control();
    assert!(control.catch_up_snapshot().topics.iter().any(|topic| {
        topic.topic == Topic::new_group_message(group.group_id)
            && topic.registration == IncomingRegistration::Pending
    }));
    // The barrier stays closed until local delivery has returned.
    let delivered = next_application(&mut reader).await?;
    assert_eq!(delivered.decrypted_message_bytes, b"already stored");
    assert!(
        control
            .catch_up_snapshot()
            .topics
            .iter()
            .all(|topic| { topic.registration != IncomingRegistration::Active })
    );
    release.notify_one();
    xmtp_common::wait_for_eq(
        || async {
            control.catch_up_snapshot().topics.iter().any(|topic| {
                topic.topic == Topic::new_group_message(group.group_id)
                    && topic.registration == IncomingRegistration::Active
            })
        },
        true,
    )
    .await?;
    reader.close();
    client.close().await?;
}

// verifies: PROC-049
#[xmtp_common::test(unwrap_try = true)]
async fn ending_reader_preserves_other_network_interest() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix.create_group(None, None)?;
    group.invite(&bo).await?;
    bo.sync_welcomes().await?;
    let mut default = MessageReader::new(
        bo.context.clone(),
        DeliveryScope::Groups(vec![group.group_id]),
        LocalDeliveryFilter::default(),
        None,
    )?;
    let mut replay = MessageReader::new(
        bo.context.clone(),
        DeliveryScope::Groups(vec![group.group_id]),
        LocalDeliveryFilter::default(),
        Some(bo.context.db().current_delivery_cursor()?),
    )?;
    let control = replay.control();
    xmtp_common::wait_for_eq(
        || async {
            control.catch_up_snapshot().topics.iter().any(|topic| {
                topic.topic == Topic::new_group_message(group.group_id)
                    && topic.registration == IncomingRegistration::Active
            })
        },
        true,
    )
    .await?;
    assert_eq!(
        bo.context.incoming_runtime().active_lease_count_for_test(),
        2
    );
    default.close();
    drop(default);
    assert_eq!(
        bo.context.incoming_runtime().active_lease_count_for_test(),
        1
    );
    // This envelope does not exist when the first reader releases its interest.
    let id = group
        .send_message(b"after reader end", Default::default())
        .await?;
    let delivered = next_application(&mut replay).await?;
    assert_eq!(delivered.id, id);
    assert_eq!(delivered.decrypted_message_bytes, b"after reader end");
    assert!(bo.context.db().get_group_message(&id)?.is_some());
    assert!(control.catch_up_snapshot().topics.iter().any(|topic| {
        topic.topic == Topic::new_group_message(group.group_id)
            && topic.registration == IncomingRegistration::Active
    }));
    replay.close();
}
