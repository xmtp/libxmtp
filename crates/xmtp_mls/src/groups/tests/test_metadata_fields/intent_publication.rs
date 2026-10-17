//! Queued metadata intent publication and failure causes.

use super::*;

/// A queued field write is authorized again when its commit is built, so
/// a write the committed policies deny by then fails its intent instead of
/// publishing a commit every receiver rejects.
// verifies: META-071
#[xmtp_common::test(unwrap_try = true)]
async fn test_denied_writes_fail_when_published() {
    tester!(alix, configured: |c| c.application_components = catalogue());
    tester!(bo);
    alix.create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.pop()?;
    let writes = vec![FieldWrite {
        component_id: ComponentId::new(TOPIC),
        component_type: ComponentType::String,
        operation: WriteOperation::Update(b"x".to_vec()),
    }];
    let own = inbox(&bo);
    let error = state_write(bo_group.context.mls_storage(), |tx| {
        tx.with_group(bo_group.group_id, |mls_group, storage| {
            apply_app_data_update_intent(
                storage,
                mls_group,
                AppDataUpdateIntentData::Fields(writes),
                own,
                &[],
                &bo_group.context.identity().installation_keys,
                false,
            )
            .map(|_| Continue(()))
        })
    })
    .unwrap_err();
    assert!(matches!(
        field_error(error),
        FieldError::Denied(id) if id.as_u16() == TOPIC
    ));
}

/// Queue `edit` of the registry entry of `id` as `group`'s super admin.
fn queue_registry_edit<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    id: u16,
    edit: impl FnOnce(&mut ComponentMetadata),
) -> Result<ID, GroupError> {
    let id = ComponentId::new(id);
    let payload = group.with_group_snapshot(|mls_group| {
        let mut metadata = load_component_registry(mls_group)?
            .get(&id)
            .unwrap()
            .unwrap();
        edit(&mut metadata);
        let delta = TlsMapDelta::new().update(id, VLBytes::new(metadata.encode_to_vec()));
        <ComponentRegistryComponent as Component>::encode_mutation(&delta)
            .map_err(|error| GroupError::ComponentSource(error.into()))
    })?;
    let intent = QueueIntent::app_data_update()
        .data(Vec::<u8>::from(AppDataUpdateIntentData::new(
            ComponentId::COMPONENT_REGISTRY.as_u16(),
            payload,
        )))
        .queue(group)?;
    Ok(intent.id)
}

/// Commit `edit` of the registry entry of `id` as `group`'s super admin.
async fn edit_registry<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    id: u16,
    edit: impl FnOnce(&mut ComponentMetadata),
) -> Result<(), GroupError> {
    let intent = queue_registry_edit(group, id, edit)?;
    group.sync_until_intent_resolved(intent).await.map(drop)
}

fn retype_status(metadata: &mut ComponentMetadata) {
    metadata.component_type = ComponentType::Bytes as i32;
}

/// Queue a write of `payload` to `id` as a `component_type` field, without
/// the checks a public write makes first.
fn queue_write<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    id: u16,
    component_type: ComponentType,
    payload: &[u8],
) -> Result<(ID, Vec<FieldWrite>), GroupError> {
    let writes = vec![FieldWrite {
        component_id: ComponentId::new(id),
        component_type,
        operation: WriteOperation::Update(payload.to_vec()),
    }];
    let intent = QueueIntent::app_data_update()
        .data(Vec::<u8>::from(AppDataUpdateIntentData::Fields(
            writes.clone(),
        )))
        .queue(group)?;
    Ok((intent.id, writes))
}

fn intent_state<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    id: ID,
) -> Result<IntentState, GroupError> {
    let intent: Option<StoredGroupIntent> = group.context.db().fetch(&id)?;
    Ok(intent.unwrap().state)
}

/// Assert that `error` is `TypeChanged` of `id`, written as `expected` and
/// committed as `actual`.
#[track_caller]
fn assert_type_changed(
    error: GroupError,
    id: u16,
    expected: ComponentType,
    actual: MetadataComponentType,
) {
    let error = field_error(error);
    assert!(
        matches!(
            &error,
            FieldError::TypeChanged {
                component_id,
                expected: e,
                actual: a,
            } if component_id.as_u16() == id && *e == expected && *a == actual
        ),
        "{error:?}"
    );
}

/// A queued field write is resolved again when it is published, so a
/// commit that lands first and re-types the field or tightens its policy
/// fails the write with that field error rather than a failed sync.
// verifies: META-071
#[xmtp_common::test(unwrap_try = true)]
async fn test_writes_refused_at_publish_keep_their_field_error() {
    tester!(alix, configured: |c| c.application_components = catalogue());
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.pop()?;
    let own = inbox(&bo);

    let (intent, writes) = queue_write(&bo_group, STATUS, ComponentType::String, b"away")?;
    edit_registry(&group, STATUS, retype_status).await?;
    let error = bo_group
        .publish_field_writes(intent, own, &writes)
        .await
        .unwrap_err();
    assert_type_changed(
        error,
        STATUS,
        ComponentType::String,
        MetadataComponentType::Bytes,
    );

    let (intent, writes) = queue_write(&bo_group, STATUS, ComponentType::Bytes, b"away")?;
    edit_registry(&group, STATUS, |metadata| {
        let admin = Some(MetadataPolicy {
            kind: Some(MetadataPolicyKind::Base(
                MetadataBasePolicy::AllowIfAdmin as i32,
            )),
        });
        metadata.permissions = Some(ComponentPermissions {
            insert_policy: admin.clone(),
            update_policy: admin.clone(),
            delete_policy: admin,
        });
    })
    .await?;
    let error = bo_group
        .publish_field_writes(intent, own, &writes)
        .await
        .unwrap_err();
    assert!(matches!(
        field_error(error),
        FieldError::Denied(id) if id.as_u16() == STATUS
    ));
    bo_group.sync().await?;
    assert_eq!(bo_group.metadata_value(&status())?, None);
}

/// The publisher refuses every queued write that no longer resolves in one
/// pass but reports only the first, so each write reports its own field
/// error, not another write's.
// verifies: META-071
#[xmtp_common::test(unwrap_try = true)]
async fn test_each_refused_write_reports_its_own_field_error() {
    tester!(alix, configured: |c| c.application_components = catalogue());
    let group = alix.create_group(None, None)?;
    // The first sync checks the installations, which publishes every
    // queued intent. Later syncs publish one commit each. The sync that
    // starts a public write sends the key update, so the write is checked
    // before the re-type lands and queued behind a TOPIC write that is
    // already invalid. Its publish then sends the re-type and refuses both
    // writes in one pass.
    group.sync().await?;
    QueueIntent::key_update().queue(&group)?;
    queue_registry_edit(&group, STATUS, retype_status)?;
    let (topic, topic_writes) = queue_write(&group, TOPIC, ComponentType::Bytes, b"news")?;

    let error = group
        .update_metadata_field(&status(), &ComponentMutation::Replace(string("away")))
        .await
        .unwrap_err();
    assert_type_changed(
        error,
        STATUS,
        ComponentType::String,
        MetadataComponentType::Bytes,
    );
    let status_write = group
        .context
        .db()
        .find_group_intents(group.group_id, None, None)?
        .pop()?;
    assert_eq!(status_write.state, IntentState::Error);
    assert_eq!(intent_state(&group, topic)?, IntentState::Error);
    let error = group
        .publish_field_writes(topic, inbox(&alix), &topic_writes)
        .await
        .unwrap_err();
    assert_type_changed(
        error,
        TOPIC,
        ComponentType::Bytes,
        MetadataComponentType::String,
    );
    assert_eq!(group.metadata_value(&status())?, None);
}

/// A write that another sync refused before its own publish ran still
/// reports its field error.
// verifies: META-071
#[xmtp_common::test(unwrap_try = true)]
async fn test_writes_refused_by_another_sync_keep_their_field_error() {
    tester!(alix, configured: |c| c.application_components = catalogue());
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.pop()?;
    edit_registry(&group, STATUS, retype_status).await?;
    bo_group.sync().await?;

    let (intent, writes) = queue_write(&bo_group, STATUS, ComponentType::String, b"away")?;
    let _ = bo_group.sync().await;
    assert_eq!(intent_state(&bo_group, intent)?, IntentState::Error);
    let error = bo_group
        .publish_field_writes(intent, inbox(&bo), &writes)
        .await
        .unwrap_err();
    assert_type_changed(
        error,
        STATUS,
        ComponentType::String,
        MetadataComponentType::Bytes,
    );
}

/// A write whose publish fails while it is still queued has not failed,
/// so it reports the sync error, not the field error it would fail with.
// verifies: META-071
#[xmtp_common::test(unwrap_try = true)]
async fn test_queued_writes_keep_their_sync_error() {
    tester!(alix, configured: |c| c.application_components = catalogue());
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.pop()?;
    edit_registry(&group, STATUS, retype_status).await?;
    bo_group.sync().await?;

    let (intent, writes) = queue_write(&bo_group, STATUS, ComponentType::String, b"away")?;
    bo.context.server_configuration().block_connection(
        crate::server_configuration::BlockedConnection::BackendMismatch {
            stored: "old.example".into(),
            received: "new.example".into(),
        },
    );
    let error = bo_group
        .publish_field_writes(intent, inbox(&bo), &writes)
        .await
        .unwrap_err();
    assert!(!matches!(error, GroupError::MetadataField(_)), "{error:?}");
    assert_eq!(intent_state(&bo_group, intent)?, IntentState::ToPublish);
}
