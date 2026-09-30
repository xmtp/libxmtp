//! Typed metadata fields and user data through the group API.

use std::collections::BTreeMap;

use prost::Message as _;
use tls_codec::{Serialize as _, VLBytes};
use xmtp_configuration::ApplicationComponentDefinition;
use xmtp_db::{
    Fetch,
    TransactionOutcome::Continue,
    group_intent::{ID, IntentState, QueryGroupIntent, StoredGroupIntent},
};
use xmtp_mls_common::{
    app_data::{
        component_id::ComponentId,
        components::tls_map_components::ComponentRegistryComponent,
        fields::{
            ComponentMutation, FieldError, FieldKey, FieldValue, FieldWrite, MapEntry, MapMutation,
            MetadataComponentType, MetadataFieldRef, MetadataKeyType, MetadataScalarType,
            MetadataValue, UserFieldUpdate, UserFieldValue, WriteOperation,
        },
        typed::Component,
    },
    inbox_id::InboxId,
    tls_map::TlsMapDelta,
};
use xmtp_proto::xmtp::mls::message_contents::{
    ComponentMetadata, ComponentPermissions, ComponentType, MetadataPolicy,
    metadata_policy::{Kind as MetadataPolicyKind, MetadataBasePolicy},
};

use super::test_dictionary_creation::definition;
use crate::{
    context::XmtpSharedContext,
    groups::{
        GroupError, MlsGroup,
        app_data::{load_component_registry, sender_intents::apply_app_data_update_intent},
        intents::{AppDataUpdateIntentData, QueueIntent},
    },
    state_tx::state_write,
    tester,
};

/// A string field in groups and DMs.
const STATUS: u16 = 0xC001;
/// A per-user string field in groups only.
const NICKNAME: u16 = 0xC002;
/// A string field only admins may write.
const TOPIC: u16 = 0xC003;
/// A per-user string field only admins may write, outside `catalogue()`.
const BADGE: u16 = 0xC004;

fn status() -> MetadataFieldRef {
    MetadataFieldRef::new(ComponentId::new(STATUS))
}

/// `NICKNAME` as its creator's catalogue names it.
fn nickname() -> MetadataFieldRef {
    MetadataFieldRef {
        component_id: ComponentId::new(NICKNAME),
        name: Some("app.c002".into()),
    }
}

fn catalogue() -> Vec<ApplicationComponentDefinition> {
    vec![
        definition(
            STATUS,
            ComponentType::String,
            MetadataBasePolicy::Allow,
            true,
            true,
        ),
        definition(
            NICKNAME,
            ComponentType::TlsMapInboxIdString,
            MetadataBasePolicy::AllowIfSelfOrNonMember,
            true,
            false,
        ),
        definition(
            TOPIC,
            ComponentType::String,
            MetadataBasePolicy::AllowIfAdmin,
            true,
            true,
        ),
    ]
}

/// Another client's catalogue entry for `STATUS`.
fn renamed(component_type: ComponentType, name: &str) -> ApplicationComponentDefinition {
    ApplicationComponentDefinition {
        name: name.into(),
        ..definition(
            STATUS,
            component_type,
            MetadataBasePolicy::Allow,
            true,
            true,
        )
    }
}

fn inbox<C: XmtpSharedContext>(client: &crate::Client<C>) -> InboxId {
    InboxId::from_hex(client.inbox_id()).unwrap()
}

fn string(s: &str) -> FieldValue {
    FieldValue::String(s.into())
}

fn set(field: MetadataFieldRef, value: &str) -> UserFieldUpdate {
    UserFieldUpdate {
        field,
        value: Some(string(value)),
    }
}

fn clear(field: MetadataFieldRef) -> UserFieldUpdate {
    UserFieldUpdate { field, value: None }
}

fn user_value(field: MetadataFieldRef, value: &str) -> UserFieldValue {
    UserFieldValue {
        field,
        value: string(value),
    }
}

fn field_error(error: GroupError) -> FieldError {
    match error {
        GroupError::MetadataField(error) => error,
        other => panic!("expected a field error, got {other:?}"),
    }
}

/// A group lists its public well-known fields with their protocol types and its
/// registered application fields named by the reader's catalogue; a DM
/// lists only the fields registered for DMs. Internal components are never
/// fields, and a name lookup prefers the well-known field.
// verifies: META-069
#[xmtp_common::test(unwrap_try = true)]
async fn test_fields_describe_the_group_registry() {
    tester!(alix, configured: |c| c.application_components = catalogue());
    tester!(bo, configured: |c| c.application_components = vec![renamed(
        ComponentType::String,
        "GROUP_NAME"
    )]);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.sync_welcomes().await?.pop()?;
    let described = |group: &MlsGroup<_>| -> Result<Vec<_>, GroupError> {
        Ok(group
            .metadata_fields()?
            .into_iter()
            .map(|f| (f.field, f.component_type))
            .collect())
    };
    let string_map = MetadataComponentType::Map {
        key_type: MetadataKeyType::InboxId,
        value_type: MetadataScalarType::String,
    };
    let well_known = [
        (MetadataFieldRef::GROUP_NAME, MetadataComponentType::String),
        (
            MetadataFieldRef::GROUP_DESCRIPTION,
            MetadataComponentType::String,
        ),
        (
            MetadataFieldRef::GROUP_IMAGE_URL,
            MetadataComponentType::String,
        ),
        (
            MetadataFieldRef::MESSAGE_DISAPPEAR_FROM_NS,
            MetadataComponentType::Bytes,
        ),
        (
            MetadataFieldRef::MESSAGE_DISAPPEAR_IN_NS,
            MetadataComponentType::Bytes,
        ),
        (MetadataFieldRef::APP_DATA, MetadataComponentType::String),
        (MetadataFieldRef::USER_DISPLAY_NAME, string_map),
        (MetadataFieldRef::GROUP_IMAGE, MetadataComponentType::Bytes),
    ];
    let named = |id: u16| MetadataFieldRef {
        component_id: ComponentId::new(id),
        name: Some(format!("app.{id:x}").into()),
    };
    let expected: Vec<_> = well_known
        .iter()
        .cloned()
        .chain([
            (named(STATUS), MetadataComponentType::String),
            (nickname(), string_map),
            (named(TOPIC), MetadataComponentType::String),
        ])
        .collect();
    assert_eq!(described(&group)?, expected);
    assert_eq!(
        group.metadata_field("app.c002")?.map(|f| f.is_user_field()),
        Some(true)
    );

    // Bo's catalogue names STATUS after a well-known field and lists
    // nothing else, so the other application fields are unnamed.
    let bo_named = MetadataFieldRef {
        component_id: ComponentId::new(STATUS),
        name: Some("GROUP_NAME".into()),
    };
    let bo_expected: Vec<_> = well_known
        .iter()
        .cloned()
        .chain([
            (bo_named, MetadataComponentType::String),
            (
                MetadataFieldRef::new(ComponentId::new(NICKNAME)),
                string_map,
            ),
            (
                MetadataFieldRef::new(ComponentId::new(TOPIC)),
                MetadataComponentType::String,
            ),
        ])
        .collect();
    assert_eq!(described(&bo_group)?, bo_expected);
    assert_eq!(
        bo_group.metadata_field("GROUP_NAME")?.map(|f| f.field),
        Some(MetadataFieldRef::GROUP_NAME)
    );
    assert!(bo_group.metadata_field("app.c001")?.is_none());

    let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    let dm_ids: Vec<_> = dm
        .metadata_fields()?
        .into_iter()
        .map(|f| f.field.component_id.as_u16())
        .collect();
    assert!(dm_ids.contains(&STATUS) && dm_ids.contains(&TOPIC));
    assert!(!dm_ids.contains(&NICKNAME));
    assert!(dm_ids.iter().all(|id| {
        ![
            ComponentId::COMPONENT_REGISTRY,
            ComponentId::SUPER_ADMIN_LIST,
            ComponentId::ADMIN_LIST,
            ComponentId::GROUP_MEMBERSHIP,
            ComponentId::DM_MEMBERS,
            ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION,
        ]
        .contains(&ComponentId::new(*id))
    }));
}

/// A member whose backend snapshot types an application field differently
/// from the group writes it under the group's type: the snapshot only
/// names the field.
// verifies: META-069, META-071
#[xmtp_common::test(unwrap_try = true)]
async fn test_stale_snapshot_uses_the_group_type() {
    tester!(alix, configured: |c| c.application_components = catalogue());
    tester!(bo, configured: |c| c.application_components = vec![renamed(
        ComponentType::Bytes,
        "mood"
    )]);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.sync_welcomes().await?.pop()?;

    let field = bo_group.metadata_field("mood")??;
    assert_eq!(field.field.component_id.as_u16(), STATUS);
    assert_eq!(field.component_type, MetadataComponentType::String);
    bo_group
        .update_metadata_field(&field.field, &ComponentMutation::Replace(string("away")))
        .await?;
    group.sync().await?;
    assert_eq!(
        group.metadata_value(&status())?,
        Some(MetadataValue::Scalar(string("away")))
    );

    let epoch = bo_group.epoch().await?;
    let bytes = ComponentMutation::Replace(FieldValue::Bytes(b"away".to_vec()));
    let error = bo_group.update_metadata_field(&field.field, &bytes).await;
    assert!(matches!(
        field_error(error.unwrap_err()),
        FieldError::TypeMismatch(id) if id.as_u16() == STATUS
    ));
    assert_eq!(bo_group.epoch().await?, epoch);
}

/// A write that matches the writer's local, unsynced state is still
/// committed once the writer has synced, because another member may have
/// changed the field since. Dropping it locally would report success for
/// a value the group never gets.
// verifies: META-071
#[xmtp_common::test(unwrap_try = true)]
async fn test_unchanged_write_against_stale_state_lands() {
    tester!(alix, configured: |c| c.application_components = catalogue());
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.sync_welcomes().await?.pop()?;
    let rename = |name| ComponentMutation::Replace(string(name));
    let name = MetadataFieldRef::GROUP_NAME;
    group.update_metadata_field(&name, &rename("Team")).await?;
    bo_group.sync().await?;
    group.update_metadata_field(&name, &rename("Other")).await?;

    bo_group
        .update_metadata_field(&name, &rename("Team"))
        .await?;
    group.sync().await?;
    assert_eq!(group.group_name()?, "Team");
    assert_eq!(bo_group.group_name()?, "Team");
}

/// A field write commits a value of the field's type, by any name. A write
/// of the current value commits nothing. A value of the wrong type, an
/// unlisted field, or a write the committed policies deny fails before any
/// commit.
// verifies: META-070, META-071
#[xmtp_common::test(unwrap_try = true)]
async fn test_update_metadata_field() {
    tester!(alix, configured: |c| c.application_components = catalogue());
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.sync_welcomes().await?.pop()?;

    let renamed = MetadataFieldRef {
        component_id: ComponentId::GROUP_NAME,
        name: Some("title".into()),
    };
    bo_group
        .update_metadata_field(&renamed, &ComponentMutation::Replace(string("Team")))
        .await?;
    group.sync().await?;
    assert_eq!(group.group_name()?, "Team");
    let values = group.metadata_values(&[renamed.clone(), MetadataFieldRef::GROUP_DESCRIPTION])?;
    assert_eq!(values[0].field, MetadataFieldRef::GROUP_NAME);
    assert_eq!(values[0].value, Some(MetadataValue::Scalar(string("Team"))));
    assert_eq!(values[1].value, None);

    let epoch = group.epoch().await?;
    group
        .update_metadata_field(
            &MetadataFieldRef::GROUP_NAME,
            &ComponentMutation::Replace(string("Team")),
        )
        .await?;
    for (field, mutation, expected) in [
        (
            MetadataFieldRef::GROUP_NAME,
            ComponentMutation::Replace(FieldValue::Bytes(b"x".to_vec())),
            FieldError::TypeMismatch(ComponentId::GROUP_NAME),
        ),
        (
            MetadataFieldRef::new(ComponentId::new(0xC0FF)),
            ComponentMutation::Remove,
            FieldError::UnknownField(ComponentId::new(0xC0FF)),
        ),
        (
            MetadataFieldRef::new(ComponentId::ADMIN_LIST),
            ComponentMutation::Remove,
            FieldError::UnknownField(ComponentId::ADMIN_LIST),
        ),
    ] {
        let error = field_error(
            group
                .update_metadata_field(&field, &mutation)
                .await
                .unwrap_err(),
        );
        assert_eq!(error.to_string(), expected.to_string());
    }
    assert_eq!(group.epoch().await?, epoch);

    // A write the committed policies deny fails before anything is
    // published: an admin-only field, and a whole-component delete of a
    // self-owned user field.
    let topic = MetadataFieldRef::new(ComponentId::new(TOPIC));
    bo_group
        .update_user_data(&[set(MetadataFieldRef::USER_DISPLAY_NAME, "Bo")])
        .await?;
    let epoch = bo_group.epoch().await?;
    for (field, mutation) in [
        (topic.clone(), ComponentMutation::Replace(string("x"))),
        (
            MetadataFieldRef::USER_DISPLAY_NAME,
            ComponentMutation::Remove,
        ),
    ] {
        let error = field_error(
            bo_group
                .update_metadata_field(&field, &mutation)
                .await
                .unwrap_err(),
        );
        assert!(matches!(error, FieldError::Denied(id) if id == field.component_id));
    }
    assert_eq!(bo_group.epoch().await?, epoch);
    group.sync().await?;
    assert_eq!(group.metadata_value(&topic)?, None);
}

/// `update_user_data` sets and clears the caller's own entries of several
/// user fields in one commit. A batch with any invalid value commits
/// nothing, and a batch that changes nothing makes no commit.
// verifies: META-073
#[xmtp_common::test(unwrap_try = true)]
async fn test_update_user_data_is_atomic() {
    tester!(alix, configured: |c| c.application_components = catalogue());
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.sync_welcomes().await?.pop()?;
    let names = MetadataFieldRef::USER_DISPLAY_NAME;

    let epoch = bo_group.epoch().await?;
    bo_group
        .update_user_data(&[set(names.clone(), "Bo"), set(nickname(), "B")])
        .await?;
    assert_eq!(bo_group.epoch().await?, epoch + 1);
    group.sync().await?;
    let bo_data = |group: &MlsGroup<_>| -> Result<_, GroupError> {
        Ok(group
            .user_data(None, Some(&[inbox(&bo)]))?
            .remove(&inbox(&bo))?)
    };
    assert_eq!(
        bo_data(&group)?,
        vec![user_value(names.clone(), "Bo"), user_value(nickname(), "B")]
    );

    // An oversize nickname fails the batch, so the display name is kept.
    let oversize = "n".repeat(8193);
    let error = bo_group
        .update_user_data(&[set(names.clone(), "Bobby"), set(nickname(), &oversize)])
        .await
        .unwrap_err();
    assert!(matches!(field_error(error), FieldError::Component(_)));
    let error = bo_group
        .update_user_data(&[set(names.clone(), "Bobby"), clear(names.clone())])
        .await
        .unwrap_err();
    assert!(matches!(
        field_error(error),
        FieldError::DuplicateField(ComponentId::USER_DISPLAY_NAME)
    ));
    assert_eq!(bo_group.epoch().await?, epoch + 1);

    // A present entry is updated and cleared in one commit.
    bo_group
        .update_user_data(&[set(names.clone(), "Bobby"), clear(nickname())])
        .await?;
    assert_eq!(bo_group.epoch().await?, epoch + 2);
    group.sync().await?;
    assert_eq!(bo_data(&group)?, vec![user_value(names, "Bobby")]);

    bo_group.update_user_data(&[clear(nickname())]).await?;
    bo_group.update_user_data(&[]).await?;
    assert_eq!(bo_group.epoch().await?, epoch + 2);
    assert_eq!(
        group.user_data(None, None)?,
        BTreeMap::from([
            (inbox(&alix), vec![]),
            (
                inbox(&bo),
                vec![user_value(MetadataFieldRef::USER_DISPLAY_NAME, "Bobby")]
            ),
        ])
    );
}

/// User data defaults to the current members, may name a former member
/// explicitly, and selects nothing for empty filters. A DM defaults to
/// its pair.
// verifies: META-072
#[xmtp_common::test(unwrap_try = true)]
async fn test_user_data_selection() {
    tester!(alix, configured: |c| c.application_components = catalogue());
    tester!(bo);
    tester!(carol);
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), carol.inbox_id()], None, None)
        .await?;
    let carol_group = carol.sync_welcomes().await?.pop()?;
    carol_group
        .update_user_data(&[set(nickname(), "C")])
        .await?;
    group.sync().await?;
    assert_eq!(
        group.user_data(Some(&[nickname()]), None)?,
        BTreeMap::from([
            (inbox(&alix), vec![]),
            (inbox(&bo), vec![]),
            (inbox(&carol), vec![user_value(nickname(), "C")]),
        ])
    );

    group.remove_members(&[carol.inbox_id()]).await?;
    let mut members = vec![inbox(&alix), inbox(&bo)];
    members.sort();
    assert_eq!(
        group.user_data(None, None)?.into_keys().collect::<Vec<_>>(),
        members
    );
    assert!(
        group
            .user_data(None, Some(&[inbox(&carol)]))?
            .contains_key(&inbox(&carol))
    );
    assert!(group.user_data(None, Some(&[]))?.is_empty());
    assert!(
        group
            .user_data(Some(&[]), None)?
            .values()
            .all(Vec::is_empty)
    );
    assert!(matches!(
        field_error(group.user_data(Some(&[status()]), None).unwrap_err()),
        FieldError::NotUserField(id) if id.as_u16() == STATUS
    ));

    let dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    dm.update_user_data(&[set(MetadataFieldRef::USER_DISPLAY_NAME, "Alix")])
        .await?;
    assert_eq!(
        dm.user_data(None, None)?,
        BTreeMap::from([
            (
                inbox(&alix),
                vec![user_value(MetadataFieldRef::USER_DISPLAY_NAME, "Alix")]
            ),
            (inbox(&bo), vec![]),
        ])
    );
}

/// Reads see only the committed dictionary, even while a proposal to
/// change it is pending; a write resolves the caller's own entry against
/// the pending value its commit will build on.
// verifies: META-070, META-073
#[xmtp_common::test(unwrap_try = true)]
async fn test_reads_are_committed_and_writes_see_pending_proposals() {
    tester!(alix);
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.sync_welcomes().await?.pop()?;
    let names = MetadataFieldRef::USER_DISPLAY_NAME;
    let own = inbox(&alix);
    publish_proposals(&group, own, display_name("Al")).await?;
    group.sync().await?;
    bo_group.sync().await?;
    assert!(pending_proposals(&group)? > 0);
    for member in [&group, &bo_group] {
        assert_eq!(member.metadata_value(&names)?, None);
        assert_eq!(
            member.user_data(Some(std::slice::from_ref(&names)), None)?[&own],
            vec![]
        );
    }

    // The commit applies the pending insert, then this write updates it.
    group
        .update_user_data(&[set(names.clone(), "Alix")])
        .await?;
    bo_group.sync().await?;
    assert_eq!(
        bo_group.map_value(&names, &FieldKey::InboxId(own))?,
        Some(string("Alix"))
    );
    assert_eq!(
        bo_group.metadata_value(&names)?,
        Some(MetadataValue::Map(vec![MapEntry {
            key: FieldKey::InboxId(own),
            value: string("Alix"),
        }]))
    );
}

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
    let bo_group = bo.sync_welcomes().await?.pop()?;
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
    let bo_group = bo.sync_welcomes().await?.pop()?;
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

/// A write the committed values already carry out makes no commit while
/// another member's proposal changes a different entry of the same field.
/// Here the caller clears an entry it does not have.
// verifies: META-073
#[xmtp_common::test(unwrap_try = true)]
async fn test_clearing_an_absent_entry_ignores_other_pending_entries() {
    tester!(alix);
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.sync_welcomes().await?.pop()?;
    let names = MetadataFieldRef::USER_DISPLAY_NAME;
    publish_proposals(&bo_group, inbox(&bo), display_name("Bo")).await?;
    group.sync().await?;
    assert!(pending_proposals(&group)? > 0);
    let epoch = group.epoch().await?;

    group.update_user_data(&[clear(names.clone())]).await?;
    assert_eq!(group.epoch().await?, epoch);
    assert_eq!(group.metadata_value(&names)?, None);
}

/// As above, when the caller sets the value it already has.
// verifies: META-073
#[xmtp_common::test(unwrap_try = true)]
async fn test_rewriting_a_committed_entry_ignores_other_pending_entries() {
    tester!(alix);
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.sync_welcomes().await?.pop()?;
    let names = MetadataFieldRef::USER_DISPLAY_NAME;
    group.update_user_data(&[set(names.clone(), "Al")]).await?;
    bo_group.sync().await?;
    publish_proposals(&bo_group, inbox(&bo), display_name("Bo")).await?;
    group.sync().await?;
    assert!(pending_proposals(&group)? > 0);
    let epoch = group.epoch().await?;

    group.update_user_data(&[set(names.clone(), "Al")]).await?;
    assert_eq!(group.epoch().await?, epoch);
    assert_eq!(
        group.map_value(&names, &FieldKey::InboxId(inbox(&bo)))?,
        None
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
    let bo_group = bo.sync_welcomes().await?.pop()?;
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
    let bo_group = bo.sync_welcomes().await?.pop()?;
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

/// Several writes the committed values already carry out, one unchanged
/// and one a clear of an absent entry, make no commit next to another
/// member's pending proposal.
// verifies: META-073
#[xmtp_common::test(unwrap_try = true)]
async fn test_unchanged_writes_ignore_other_pending_entries() {
    tester!(alix, configured: |c| c.application_components = catalogue());
    tester!(bo);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.sync_welcomes().await?.pop()?;
    let names = MetadataFieldRef::USER_DISPLAY_NAME;
    group.update_user_data(&[set(names.clone(), "Al")]).await?;
    bo_group.sync().await?;
    publish_proposals(&bo_group, inbox(&bo), display_name("Bo")).await?;
    group.sync().await?;
    assert!(pending_proposals(&group)? > 0);
    let epoch = group.epoch().await?;

    group
        .update_user_data(&[set(names.clone(), "Al"), clear(nickname())])
        .await?;
    assert_eq!(group.epoch().await?, epoch);
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
    let bo_group = bo.sync_welcomes().await?.pop()?;
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
    let bo_group = bo.sync_welcomes().await?.pop()?;
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
    let bo_group = bo.sync_welcomes().await?.pop()?;
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
    let bo_group = bo.sync_welcomes().await?.pop()?;
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
    let bo_group = bo.sync_welcomes().await?.pop()?;
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
    let bo_group = bo.sync_welcomes().await?.pop()?;
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

/// The number of proposals `group` holds pending.
fn pending_proposals<C: XmtpSharedContext>(group: &MlsGroup<C>) -> Result<usize, GroupError> {
    group.with_group_snapshot(|group| Ok(group.pending_proposals().count()))
}

/// A write of `value` to the writer's own display name.
fn display_name(value: &str) -> Vec<FieldWrite> {
    vec![FieldWrite {
        component_id: ComponentId::USER_DISPLAY_NAME,
        component_type: ComponentType::TlsMapInboxIdString,
        operation: WriteOperation::SetOwn(value.as_bytes().to_vec()),
    }]
}

/// Queue `writes` by `own` as one intent and publish it, without the
/// checks a public write makes first.
async fn publish_writes<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    own: InboxId,
    writes: Vec<FieldWrite>,
) -> Result<(), GroupError> {
    let intent = QueueIntent::app_data_update()
        .data(Vec::<u8>::from(AppDataUpdateIntentData::Fields(
            writes.clone(),
        )))
        .queue(group)?;
    group.publish_field_writes(intent.id, own, &writes).await
}

/// Publish only the proposals of `writes` by `own`, so a member that syncs
/// holds them pending with no commit.
async fn publish_proposals<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    own: InboxId,
    writes: Vec<FieldWrite>,
) -> Result<(), GroupError> {
    let mut payloads = state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls_group, storage| {
            let publish = apply_app_data_update_intent(
                storage,
                mls_group,
                AppDataUpdateIntentData::Fields(writes),
                own,
                &[],
                &group.context.identity().installation_keys,
                false,
            )?;
            Ok::<_, GroupError>(Continue(publish.expect("a commit").payloads_to_publish))
        })
    })?
    .into_continued();
    payloads.pop().expect("the commit");
    let messages = group.prepare_group_messages(
        payloads
            .iter()
            .map(|payload| (payload.as_slice(), false))
            .collect(),
    )?;
    group.context.api().send_group_messages(messages).await?;
    Ok(())
}

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
    let bo_group = bo.sync_welcomes().await?.pop()?;
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
    let bo_group = bo.sync_welcomes().await?.pop()?;
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
    let bo_group = bo.sync_welcomes().await?.pop()?;
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
    let bo_group = bo.sync_welcomes().await?.pop()?;
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
