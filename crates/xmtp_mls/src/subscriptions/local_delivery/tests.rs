use super::*;
use crate::{test::mock::generate_stored_msg, tester};
use futures::{FutureExt, StreamExt};
use xmtp_common::time::{Duration, timeout};
use xmtp_db::{ConnectionExt, Store};
use xmtp_proto::types::Cursor;

#[xmtp_common::test(unwrap_try = true)]
async fn iterator_drop_retains_the_last_item_until_a_later_next_request() {
    tester!(alix);
    let group = alix.create_group(None, None)?;
    let first = generate_stored_msg(Cursor(100), group.group_id);
    let second = generate_stored_msg(Cursor(200), group.group_id);
    first.store(&alix.context.db())?;
    second.store(&alix.context.db())?;
    let create = || {
        LocalDelivery::new(
            alix.context.clone(),
            DeliveryScope::Groups(vec![group.group_id]),
            LocalDeliveryFilter::default(),
            None,
            LocalDeliveryConfig::default(),
        )
    };
    let mut stream = Box::pin(create()?.into_stream());
    assert_eq!(stream.next().await.unwrap()?.id, first.id);
    drop(stream);
    let mut stream = Box::pin(create()?.into_stream());
    assert_eq!(stream.next().await.unwrap()?.id, first.id);
    assert_eq!(stream.next().await.unwrap()?.id, second.id);
    drop(stream);
    let mut stream = Box::pin(create()?.into_stream());
    assert_eq!(stream.next().await.unwrap()?.id, second.id);
}

#[xmtp_common::test(unwrap_try = true)]
async fn rejected_callback_releases_owner_without_consuming_its_item() {
    tester!(alix);
    let group = alix.create_group(None, None)?;
    let message = generate_stored_msg(Cursor(100), group.group_id);
    message.store(&alix.context.db())?;
    let create = || {
        LocalDelivery::new(
            alix.context.clone(),
            DeliveryScope::Groups(vec![group.group_id]),
            LocalDeliveryFilter::default(),
            None,
            LocalDeliveryConfig::default(),
        )
    };
    let mut reader = create()?;
    let item = reader.next_delivery().await?.unwrap();
    assert!(create().is_err());
    item.acknowledgement.reject();
    let mut replacement = create()?;
    assert_eq!(
        replacement.next_delivery().await?.unwrap().message.id,
        message.id
    );
    assert!(matches!(
        reader.next_delivery().await,
        Err(LocalDeliveryError::AcknowledgementRejected)
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn cancelling_a_pending_next_does_not_bypass_explicit_acknowledgement() {
    tester!(alix);
    let group = alix.create_group(None, None)?;
    let first = generate_stored_msg(Cursor(100), group.group_id);
    let second = generate_stored_msg(Cursor(200), group.group_id);
    first.store(&alix.context.db())?;
    second.store(&alix.context.db())?;
    let mut reader = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::Groups(vec![group.group_id]),
        LocalDeliveryFilter::default(),
        None,
        LocalDeliveryConfig::default(),
    )?;
    let item = reader.next_delivery().await?.unwrap();
    assert!(reader.next_delivery().now_or_never().is_none());
    assert!(reader.next_delivery().now_or_never().is_none());
    item.acknowledgement.acknowledge()?;
    assert_eq!(reader.next_delivery().await?.unwrap().message.id, second.id);
}

#[xmtp_common::test(unwrap_try = true)]
async fn excluded_rows_stay_consumed_after_a_filter_change() {
    tester!(alix);
    let denied = alix.create_group(None, None)?;
    let allowed = alix.create_group(None, None)?;
    denied.update_consent_state(ConsentState::Denied)?;
    let excluded = generate_stored_msg(Cursor(100), denied.group_id);
    let selected = generate_stored_msg(Cursor(200), allowed.group_id);
    excluded.store(&alix.context.db())?;
    selected.store(&alix.context.db())?;
    let mut reader = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::All,
        LocalDeliveryFilter {
            consent_states: Some(vec![ConsentState::Allowed]),
            ..Default::default()
        },
        None,
        LocalDeliveryConfig::default(),
    )?;
    let item = reader.next_delivery().await?.unwrap();
    assert_eq!(item.message.id, selected.id);
    item.acknowledgement.acknowledge()?;
    let later = generate_stored_msg(Cursor(300), denied.group_id);
    later.store(&alix.context.db())?;
    let candidate = alix.context.db().default_delivery_messages(
        reader.session.owner.unwrap(),
        &DeliveryScope::All,
        now_ns(),
        1,
    )?[0]
        .clone();
    let revision = reader.control.selection.lock().revision;
    // The filter rejected this candidate before its group left the selected scope.
    reader
        .control()
        .update_scope(DeliveryScope::Groups(vec![allowed.group_id]));
    assert!(!reader.skip_candidate(&candidate, revision)?);
    reader
        .control()
        .update_filter(LocalDeliveryFilter::default());
    reader.control().update_scope(DeliveryScope::All);
    assert_eq!(reader.next_delivery().await?.unwrap().message.id, later.id);
}

#[xmtp_common::test(unwrap_try = true)]
async fn missed_local_wake_is_recovered_by_a_fresh_database_poll() {
    tester!(alix);
    let group = alix.create_group(None, None)?;
    let mut reader = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::Groups(vec![group.group_id]),
        LocalDeliveryFilter::default(),
        None,
        LocalDeliveryConfig {
            poll_interval: Duration::from_millis(20),
            ..Default::default()
        },
    )?;
    assert!(reader.next_delivery().now_or_never().is_none());
    let message = generate_stored_msg(Cursor(100), group.group_id);
    // Direct storage deliberately emits no process-local wake event.
    message.store(&alix.context.db())?;
    let item = timeout(Duration::from_secs(2), reader.next_delivery())
        .await??
        .unwrap();
    assert_eq!(item.message.id, message.id);
}

#[xmtp_common::test(unwrap_try = true)]
async fn stale_host_queue_token_cannot_dispatch_or_acknowledge() {
    tester!(alix);
    let group = alix.create_group(None, None)?;
    generate_stored_msg(Cursor(100), group.group_id).store(&alix.context.db())?;
    let mut reader = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::Groups(vec![group.group_id]),
        LocalDeliveryFilter::default(),
        None,
        LocalDeliveryConfig::default(),
    )?;
    let item = reader.next_delivery().await?.unwrap();
    alix.context.db().rotate_stream_database_id()?;
    assert!(item.acknowledgement.check_owner().is_err());
    assert!(item.acknowledgement.acknowledge().is_err());
    assert!(reader.next_delivery().await.is_err());
}

#[xmtp_common::test(unwrap_try = true)]
async fn removed_and_readded_scope_discards_old_queued_tokens_without_acknowledging() {
    tester!(alix);
    let group = alix.create_group(None, None)?;
    let message = generate_stored_msg(Cursor(100), group.group_id);
    message.store(&alix.context.db())?;
    let mut reader = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::Groups(vec![group.group_id]),
        LocalDeliveryFilter::default(),
        None,
        LocalDeliveryConfig::default(),
    )?;
    let old = reader.next_delivery().await?.unwrap();
    reader.control().update_scope(DeliveryScope::Groups(vec![]));
    reader
        .control()
        .update_scope(DeliveryScope::Groups(vec![group.group_id]));
    assert!(matches!(
        old.acknowledgement.check_owner(),
        Err(LocalDeliveryError::SelectionChanged)
    ));
    assert!(matches!(
        old.acknowledgement.acknowledge(),
        Err(LocalDeliveryError::SelectionChanged)
    ));
    drop(old);
    let selected_again = reader.next_delivery().await?.unwrap();
    assert_eq!(selected_again.message.id, message.id);
    selected_again.acknowledgement.check_owner()?;
    reader.control().update_scope(DeliveryScope::Groups(vec![]));
    // This callback already started. Removing its group cannot undo its successful return.
    selected_again.acknowledgement.acknowledge()?;
    assert!(
        alix.context
            .db()
            .default_delivery_messages(
                reader.session.owner.unwrap(),
                &DeliveryScope::All,
                now_ns(),
                8
            )?
            .is_empty()
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn queued_content_is_rechecked_after_deletion_and_restore() {
    use diesel::{QueryDsl, RunQueryDsl};
    tester!(alix);
    let group = alix.create_group(None, None)?;
    let start = alix.context.db().current_delivery_cursor()?;
    let first = generate_stored_msg(Cursor(100), group.group_id);
    let second = generate_stored_msg(Cursor(200), group.group_id);
    first.store(&alix.context.db())?;
    second.store(&alix.context.db())?;
    let mut reader = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::Groups(vec![group.group_id]),
        LocalDeliveryFilter::default(),
        Some(start),
        LocalDeliveryConfig::default(),
    )?;
    let removed = reader.next_delivery().await?.unwrap();
    alix.context.db().raw_query(|conn| {
        diesel::delete(xmtp_db::schema::group_messages::table.find(&first.id)).execute(conn)
    })?;
    assert!(matches!(
        removed.acknowledgement.check_owner(),
        Err(LocalDeliveryError::SelectionChanged)
    ));
    drop(removed);
    let retained = reader.next_delivery().await?.unwrap();
    assert_eq!(retained.message.id, second.id);
    alix.context.db().rotate_stream_database_id()?;
    assert!(matches!(
        retained.acknowledgement.check_owner(),
        Err(LocalDeliveryError::Storage(StorageError::Stream(
            xmtp_db::stream_storage::StreamStorageError::ForeignCursor
        )))
    ));
}
