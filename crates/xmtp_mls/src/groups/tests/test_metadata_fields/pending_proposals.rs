//! Pending proposal behavior and authorization.

use super::*;

/// A write of the value a pending proposal already sets commits that
/// proposal, so its success means the value is committed, as reads see it.
// verifies: META-073
#[xmtp_common::test(unwrap_try = true)]
async fn test_writes_carried_out_by_pending_proposals_commit_them() {
    tester!(alix);
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo
        .wait_for_welcomes()
        .await?
        .pop()
        .expect("Bo group welcome");
    let names = MetadataFieldRef::USER_DISPLAY_NAME;
    let own = inbox(&alix);
    publish_proposals(&group, own, display_name("Al")).await?;
    group.sync().await?;

    group.update_user_data(&[set(names.clone(), "Al")]).await?;
    bo_group.sync().await?;
    for member in [&group, &bo_group] {
        assert_eq!(
            member.map_value(&names, &FieldKey::InboxId(own))?,
            Some(string("Al"))
        );
    }
}

/// A queued write that a proposal received before its publish already
/// carries out commits that proposal when it is published.
// verifies: META-073
#[xmtp_common::test(unwrap_try = true)]
async fn test_queued_writes_carried_out_by_pending_proposals_commit_them() {
    tester!(alix);
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo
        .wait_for_welcomes()
        .await?
        .pop()
        .expect("Bo group welcome");
    let names = MetadataFieldRef::USER_DISPLAY_NAME;
    let own = inbox(&alix);
    publish_proposals(&group, own, display_name("Al")).await?;
    group.sync().await?;

    let writes = display_name("Al");
    let intent = QueueIntent::app_data_update()
        .data(Vec::<u8>::from(AppDataUpdateIntentData::Fields(
            writes.clone(),
        )))
        .queue(&group)?;
    group.publish_field_writes(intent.id, own, &writes).await?;
    bo_group.sync().await?;
    assert_eq!(
        bo_group.map_value(&names, &FieldKey::InboxId(own))?,
        Some(string("Al"))
    );
}

/// A map update of an entry that only a pending proposal holds cannot
/// apply to the committed values, so it commits that proposal rather than
/// failing or committing nothing.
// verifies: META-071
#[xmtp_common::test(unwrap_try = true)]
async fn test_map_updates_of_pending_entries_commit_them() {
    tester!(alix);
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo
        .wait_for_welcomes()
        .await?
        .pop()
        .expect("Bo group welcome");
    let names = MetadataFieldRef::USER_DISPLAY_NAME;
    let own = inbox(&alix);
    publish_proposals(&group, own, display_name("Al")).await?;
    group.sync().await?;

    let update = MapMutation::Update(FieldKey::InboxId(own), string("Al"));
    group
        .update_metadata_field(&names, &ComponentMutation::MapDelta(vec![update]))
        .await?;
    bo_group.sync().await?;
    assert_eq!(
        bo_group.map_value(&names, &FieldKey::InboxId(own))?,
        Some(string("Al"))
    );
}

/// A pending removal of the caller's own committed entry carries out a
/// clear of it, so the clear commits that removal.
// verifies: META-073
#[xmtp_common::test(unwrap_try = true)]
async fn test_clears_carried_out_by_pending_removals_commit_them() {
    tester!(alix);
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo
        .wait_for_welcomes()
        .await?
        .pop()
        .expect("Bo group welcome");
    let names = MetadataFieldRef::USER_DISPLAY_NAME;
    let own = inbox(&alix);
    group.update_user_data(&[set(names.clone(), "Al")]).await?;
    let clear_own = vec![FieldWrite {
        component_id: ComponentId::USER_DISPLAY_NAME,
        component_type: ComponentType::TlsMapInboxIdString,
        operation: WriteOperation::ClearOwn,
    }];
    publish_proposals(&group, own, clear_own).await?;
    group.sync().await?;
    assert!(pending_proposals(&group)? > 0);

    group.update_user_data(&[clear(names.clone())]).await?;
    bo_group.sync().await?;
    assert_eq!(bo_group.map_value(&names, &FieldKey::InboxId(own))?, None);
}

/// Each public no-op keeps another member's entry pending. A committed
/// rewrite and an absent clear make no commit, alone or in one batch.
// verifies: META-073
#[xmtp_common::test(unwrap_try = true)]
async fn test_unchanged_writes_ignore_other_pending_entries_absent_clear() {
    unchanged_writes_ignore_other_pending_entries(false, true, false).await?;
}

// verifies: META-073
#[xmtp_common::test(unwrap_try = true)]
async fn test_unchanged_writes_ignore_other_pending_entries_committed_rewrite() {
    unchanged_writes_ignore_other_pending_entries(true, false, false).await?;
}

// verifies: META-073
#[xmtp_common::test(unwrap_try = true)]
async fn test_unchanged_writes_ignore_other_pending_entries_mixed_batch() {
    unchanged_writes_ignore_other_pending_entries(true, false, true).await?;
}

async fn unchanged_writes_ignore_other_pending_entries(
    committed_name: bool,
    clear_name: bool,
    clear_nickname: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    tester!(alix, configured: |c| c.application_components = catalogue());
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo
        .wait_for_welcomes()
        .await?
        .pop()
        .expect("Bo group welcome");
    let names = MetadataFieldRef::USER_DISPLAY_NAME;
    if committed_name {
        group.update_user_data(&[set(names.clone(), "Al")]).await?;
    }
    bo_group.sync().await?;
    publish_proposals(&bo_group, inbox(&bo), display_name("Bo")).await?;
    group.sync().await?;
    let pending = pending_proposals(&group)?;
    assert!(pending > 0);
    let epoch = group.epoch().await?;

    let mut writes = vec![if clear_name {
        clear(names.clone())
    } else {
        set(names.clone(), "Al")
    }];
    if clear_nickname {
        writes.push(clear(nickname()));
    }
    group.update_user_data(&writes).await?;
    assert_eq!(group.epoch().await?, epoch);
    assert_eq!(pending_proposals(&group)?, pending);
    assert_eq!(
        group.map_value(&names, &FieldKey::InboxId(inbox(&bo)))?,
        None
    );
    assert_eq!(
        group.map_value(&names, &FieldKey::InboxId(inbox(&alix)))?,
        committed_name.then(|| string("Al"))
    );
    assert_eq!(group.metadata_value(&nickname())?, None);
    if !committed_name {
        assert_eq!(group.metadata_value(&names)?, None);
    }
    Ok(())
}

/// A queued write whose pending proposal another member commits first is
/// then carried out by the committed values, so its publish makes no
/// commit.
// verifies: META-073
#[xmtp_common::test(unwrap_try = true)]
async fn test_queued_writes_committed_by_another_member_make_no_commit() {
    tester!(alix);
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo
        .wait_for_welcomes()
        .await?
        .pop()
        .expect("Bo group welcome");
    let names = MetadataFieldRef::USER_DISPLAY_NAME;
    let own = inbox(&alix);
    publish_proposals(&group, own, display_name("Al")).await?;
    group.sync().await?;
    let writes = display_name("Al");
    let intent = QueueIntent::app_data_update()
        .data(Vec::<u8>::from(AppDataUpdateIntentData::Fields(
            writes.clone(),
        )))
        .queue(&group)?;
    bo_group.sync().await?;
    bo_group
        .update_user_data(&[set(names.clone(), "Bo")])
        .await?;
    let epoch = bo_group.epoch().await?;

    group.publish_field_writes(intent.id, own, &writes).await?;
    assert_eq!(group.epoch().await?, epoch);
    assert_eq!(
        group.map_value(&names, &FieldKey::InboxId(own))?,
        Some(string("Al"))
    );
}

/// A write that only another member's pending proposal carries out is
/// still checked against the committed policies, as the change it makes
/// to the committed values: a member who may not write the field is
/// refused and commits nothing.
// verifies: META-071
#[xmtp_common::test(unwrap_try = true)]
async fn test_writes_carried_out_by_pending_proposals_are_authorized() {
    tester!(alix, configured: |c| c.application_components = catalogue());
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo
        .wait_for_welcomes()
        .await?
        .pop()
        .expect("Bo group welcome");
    let topic = vec![FieldWrite {
        component_id: ComponentId::new(TOPIC),
        component_type: ComponentType::String,
        operation: WriteOperation::Update(b"x".to_vec()),
    }];
    publish_proposals(&group, inbox(&alix), topic).await?;
    bo_group.sync().await?;
    assert!(pending_proposals(&bo_group)? > 0);
    let epoch = bo_group.epoch().await?;

    let error = bo_group
        .update_metadata_field(
            &MetadataFieldRef::new(ComponentId::new(TOPIC)),
            &ComponentMutation::Replace(string("x")),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        field_error(error),
        FieldError::Denied(id) if id.as_u16() == TOPIC
    ));
    assert_eq!(bo_group.epoch().await?, epoch);
}

/// A map update of another member's entry that only their pending proposal
/// holds is checked as that update after the proposal, so a member who may
/// not write the entry is refused.
// verifies: META-071
#[xmtp_common::test(unwrap_try = true)]
async fn test_map_updates_of_pending_entries_are_authorized() {
    tester!(alix, configured: |c| c.application_components = catalogue());
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo
        .wait_for_welcomes()
        .await?
        .pop()
        .expect("Bo group welcome");
    let own = inbox(&alix);
    let nickname_write = vec![FieldWrite {
        component_id: ComponentId::new(NICKNAME),
        component_type: ComponentType::TlsMapInboxIdString,
        operation: WriteOperation::SetOwn(b"Al".to_vec()),
    }];
    publish_proposals(&group, own, nickname_write).await?;
    bo_group.sync().await?;
    assert!(pending_proposals(&bo_group)? > 0);
    let epoch = bo_group.epoch().await?;

    let update = MapMutation::Update(FieldKey::InboxId(own), string("Al"));
    let error = bo_group
        .update_metadata_field(&nickname(), &ComponentMutation::MapDelta(vec![update]))
        .await
        .unwrap_err();
    assert!(matches!(
        field_error(error),
        FieldError::Denied(id) if id.as_u16() == NICKNAME
    ));
    assert_eq!(bo_group.epoch().await?, epoch);
}

/// A write that only another member's pending proposal carries out is
/// checked even when another write of the same call adds an operation to
/// the commit.
// verifies: META-071
#[xmtp_common::test(unwrap_try = true)]
async fn test_writes_carried_out_by_pending_proposals_are_authorized_beside_other_writes() {
    tester!(alix, configured: |c| c.application_components = catalogue());
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo
        .wait_for_welcomes()
        .await?
        .pop()
        .expect("Bo group welcome");
    let update = |id: u16, value: &[u8]| FieldWrite {
        component_id: ComponentId::new(id),
        component_type: ComponentType::String,
        operation: WriteOperation::Update(value.to_vec()),
    };
    publish_proposals(&group, inbox(&alix), vec![update(TOPIC, b"x")]).await?;
    bo_group.sync().await?;
    assert!(pending_proposals(&bo_group)? > 0);
    let epoch = bo_group.epoch().await?;

    let writes = vec![update(TOPIC, b"x"), update(STATUS, b"y")];
    let error = publish_writes(&bo_group, inbox(&bo), writes)
        .await
        .unwrap_err();
    assert!(matches!(
        field_error(error),
        FieldError::Denied(id) if id.as_u16() == TOPIC
    ));
    assert_eq!(bo_group.epoch().await?, epoch);
    assert_eq!(bo_group.metadata_value(&status())?, None);
}

/// A user field entry that only an admin's pending proposal gives the
/// caller is checked beside the caller's other user field writes, so a
/// member who may not write the field is refused and commits none of them.
// verifies: META-071
#[xmtp_common::test(unwrap_try = true)]
async fn test_user_data_carried_out_by_pending_proposals_is_authorized_beside_other_writes() {
    tester!(alix, configured: |c| c.application_components = vec![definition(
        BADGE,
        ComponentType::TlsMapInboxIdString,
        MetadataBasePolicy::AllowIfAdmin,
        true,
        false,
    )]);
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo
        .wait_for_welcomes()
        .await?
        .pop()
        .expect("Bo group welcome");
    let bo_inbox = inbox(&bo);
    let delta = TlsMapDelta::new().insert(bo_inbox, VLBytes::new(b"gold".to_vec()));
    let give_badge = vec![FieldWrite {
        component_id: ComponentId::new(BADGE),
        component_type: ComponentType::TlsMapInboxIdString,
        operation: WriteOperation::Update(delta.tls_serialize_detached()?),
    }];
    publish_proposals(&group, inbox(&alix), give_badge).await?;
    bo_group.sync().await?;
    assert!(pending_proposals(&bo_group)? > 0);
    let epoch = bo_group.epoch().await?;

    let names = MetadataFieldRef::USER_DISPLAY_NAME;
    let badge = MetadataFieldRef::new(ComponentId::new(BADGE));
    let error = bo_group
        .update_user_data(&[set(names.clone(), "Bo"), set(badge, "gold")])
        .await
        .unwrap_err();
    assert!(matches!(
        field_error(error),
        FieldError::Denied(id) if id.as_u16() == BADGE
    ));
    assert_eq!(bo_group.epoch().await?, epoch);
    assert_eq!(
        bo_group.map_value(&names, &FieldKey::InboxId(bo_inbox))?,
        None
    );
}

/// A map update of an entry that only a pending proposal holds is checked
/// as that update after the proposal even when another write of the same
/// call adds an operation: another member is refused, and the entry's owner
/// commits the call.
// verifies: META-071
#[xmtp_common::test(unwrap_try = true)]
async fn test_map_updates_of_pending_entries_are_authorized_beside_other_writes() {
    tester!(alix, configured: |c| c.application_components = catalogue());
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo
        .wait_for_welcomes()
        .await?
        .pop()
        .expect("Bo group welcome");
    let own = inbox(&alix);
    let nickname_write = vec![FieldWrite {
        component_id: ComponentId::new(NICKNAME),
        component_type: ComponentType::TlsMapInboxIdString,
        operation: WriteOperation::SetOwn(b"Al".to_vec()),
    }];
    publish_proposals(&group, own, nickname_write).await?;
    group.sync().await?;
    bo_group.sync().await?;
    assert!(pending_proposals(&bo_group)? > 0);
    let delta = TlsMapDelta::new().update(own, VLBytes::new(b"Al".to_vec()));
    let writes = vec![
        FieldWrite {
            component_id: ComponentId::new(STATUS),
            component_type: ComponentType::String,
            operation: WriteOperation::Update(b"y".to_vec()),
        },
        FieldWrite {
            component_id: ComponentId::new(NICKNAME),
            component_type: ComponentType::TlsMapInboxIdString,
            operation: WriteOperation::Update(delta.tls_serialize_detached()?),
        },
    ];

    let bo_epoch = bo_group.epoch().await?;
    let error = publish_writes(&bo_group, inbox(&bo), writes.clone())
        .await
        .unwrap_err();
    assert!(matches!(
        field_error(error),
        FieldError::Denied(id) if id.as_u16() == NICKNAME
    ));
    assert_eq!(bo_group.epoch().await?, bo_epoch);

    let epoch = group.epoch().await?;
    publish_writes(&group, own, writes).await?;
    assert_eq!(group.epoch().await?, epoch + 1);
}
