//! Local delivery tests: scope filters.

use super::*;

// verifies: CONS-043
#[xmtp_common::test(unwrap_try = true)]
async fn denied_group_with_default_filter_is_delivered_when_scoped() {
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    group.update_consent_state(ConsentState::Denied)?;
    let message = generate_stored_msg(Cursor(100), group.group_id);
    message.store(&alix.context.db())?;
    let mut reader = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::Groups(vec![group.group_id]),
        LocalDeliveryFilter::default(),
        None,
        LocalDeliveryConfig::default(),
    )?;
    let item = timeout(Duration::from_secs(5), reader.next_delivery())
        .await??
        .unwrap();
    assert_eq!(item.message.id, message.id);
}

#[xmtp_common::test(unwrap_try = true)]
async fn excluded_rows_stay_consumed_after_a_filter_change() {
    let alix = EngineTestPeer::new().await;
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
    let item = timeout(Duration::from_secs(2), reader.next_delivery())
        .await??
        .unwrap();
    assert_eq!(item.message.id, selected.id);
    item.acknowledgement.acknowledge()?;
    let later = generate_stored_msg(Cursor(300), denied.group_id);
    later.store(&alix.context.db())?;
    let candidate = alix.context.db().default_delivery_messages(
        reader.session.owner().unwrap(),
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
    reader.control().update_filter(LocalDeliveryFilter {
        consent_states: Some(vec![
            ConsentState::Allowed,
            ConsentState::Unknown,
            ConsentState::Denied,
        ]),
        ..Default::default()
    });
    reader.control().update_scope(DeliveryScope::All);
    assert_eq!(
        timeout(Duration::from_secs(2), reader.next_delivery())
            .await??
            .unwrap()
            .message
            .id,
        later.id
    );
}

// verifies: CONS-042, CONS-043
#[xmtp_common::test(unwrap_try = true)]
async fn all_scope_defaults_to_allowed_and_unknown_but_group_scope_does_not() {
    let alix = EngineTestPeer::new().await;
    let denied = alix.create_group(None, None)?;
    let allowed = alix.create_group(None, None)?;
    let unknown = alix.create_group(None, None)?;
    denied.update_consent_state(ConsentState::Denied)?;
    unknown.update_consent_state(ConsentState::Unknown)?;
    let denied_message = generate_stored_msg(Cursor(100), denied.group_id);
    let allowed_message = generate_stored_msg(Cursor(200), allowed.group_id);
    let unknown_message = generate_stored_msg(Cursor(250), unknown.group_id);
    denied_message.store(&alix.context.db())?;
    allowed_message.store(&alix.context.db())?;
    unknown_message.store(&alix.context.db())?;

    let mut all = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::All,
        LocalDeliveryFilter::default(),
        None,
        LocalDeliveryConfig::default(),
    )?;
    let selected = all.next_delivery().await?.expect("allowed message");
    assert_eq!(selected.message.id, allowed_message.id);
    selected.acknowledgement.acknowledge()?;
    let selected = timeout(Duration::from_secs(2), all.next_delivery())
        .await??
        .expect("unknown message");
    assert_eq!(selected.message.id, unknown_message.id);
    selected.acknowledgement.acknowledge()?;
    all.control().close();
    drop(all);

    let later_denied = generate_stored_msg(Cursor(300), denied.group_id);
    later_denied.store(&alix.context.db())?;
    let mut one_group = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::Groups(vec![denied.group_id]),
        LocalDeliveryFilter::default(),
        None,
        LocalDeliveryConfig::default(),
    )?;
    let selected = one_group
        .next_delivery()
        .await?
        .expect("denied group message");
    assert_eq!(selected.message.id, later_denied.id);
}

// verifies: CONS-042, CONS-043
#[xmtp_common::test(unwrap_try = true)]
async fn all_scope_history_snapshot_uses_the_stream_consent_default() {
    let alix = EngineTestPeer::new().await;
    let denied = alix.create_group(None, None)?;
    let allowed = alix.create_group(None, None)?;
    denied.update_consent_state(ConsentState::Denied)?;
    let denied_message = generate_stored_msg(Cursor(100), denied.group_id);
    let allowed_message = generate_stored_msg(Cursor(200), allowed.group_id);
    denied_message.store(&alix.context.db())?;
    allowed_message.store(&alix.context.db())?;

    let snapshot = LocalDelivery::history_snapshot(
        &alix.context,
        &DeliveryScope::All,
        &LocalDeliveryFilter::default(),
        10,
    )?;
    assert_eq!(snapshot.messages.len(), 1);
    assert_eq!(snapshot.messages[0].message.id, allowed_message.id);

    let mut reader = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::All,
        LocalDeliveryFilter::default(),
        None,
        LocalDeliveryConfig::default(),
    )?;
    let streamed = timeout(Duration::from_secs(2), reader.next_delivery())
        .await??
        .expect("allowed message");
    assert_eq!(streamed.message.id, snapshot.messages[0].message.id);

    let named_snapshot = LocalDelivery::history_snapshot(
        &alix.context,
        &DeliveryScope::Groups(vec![denied.group_id]),
        &LocalDeliveryFilter::default(),
        10,
    )?;
    assert_eq!(named_snapshot.messages.len(), 1);
    assert_eq!(named_snapshot.messages[0].message.id, denied_message.id);
}

// verifies: SYNC-005, PROC-026, PROC-034
#[xmtp_common::test(unwrap_try = true)]
async fn all_reader_and_snapshot_hide_sync_without_consuming_internal_messages() {
    use xmtp_db::{diesel::prelude::*, group::ConversationType, schema::groups};
    let alix = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    let sync = alix.create_group(None, None)?;
    alix.context.db().raw_query(|conn| {
        diesel::update(groups::table.find(sync.group_id))
            .set(groups::conversation_type.eq(ConversationType::Sync))
            .execute(conn)
    })?;
    sync.update_consent_state(ConsentState::Allowed)?;
    let mut hidden = generate_stored_msg(Cursor(100), sync.group_id);
    hidden.decrypted_message_bytes = vec![0x6b; 42];
    hidden.store(&alix.context.db())?;
    let visible = generate_stored_msg(Cursor(200), group.group_id);
    visible.store(&alix.context.db())?;
    let selection = LocalDeliveryFilter {
        consent_states: Some(vec![ConsentState::Allowed, ConsentState::Unknown]),
        ..Default::default()
    };
    let snapshot =
        LocalDelivery::history_snapshot(&alix.context, &DeliveryScope::All, &selection, 1)?;
    assert_eq!(snapshot.messages.len(), 1);
    assert_eq!(snapshot.messages[0].message.id, visible.id);
    assert_eq!(
        snapshot.cursor,
        alix.context.db().current_delivery_cursor()?
    );
    let mut reader = LocalDelivery::new(
        alix.context.clone(),
        DeliveryScope::All,
        selection,
        None,
        LocalDeliveryConfig::default(),
    )?;
    let item = timeout(Duration::from_secs(2), reader.next_delivery())
        .await??
        .unwrap();
    assert_eq!(item.message.id, visible.id);
    let explicit = alix
        .context
        .db()
        .get_group_messages(&sync.group_id, &Default::default())?;
    assert!(explicit.iter().any(|row| row.id == hidden.id));
    item.acknowledgement.reject();
    reader.close();
}
