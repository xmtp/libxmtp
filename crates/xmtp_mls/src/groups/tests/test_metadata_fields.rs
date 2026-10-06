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
    let bo_group = bo.wait_for_welcomes().await?.pop()?;
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
    let bo_group = bo.wait_for_welcomes().await?.pop()?;

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
    let bo_group = bo.wait_for_welcomes().await?.pop()?;
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
    let bo_group = bo.wait_for_welcomes().await?.pop()?;

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
    let bo_group = bo.wait_for_welcomes().await?.pop()?;
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
    let carol_group = carol.wait_for_welcomes().await?.pop()?;
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
    let bo_group = bo.wait_for_welcomes().await?.pop()?;
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

mod fixtures;
mod intent_publication;
mod pending_proposals;

use fixtures::*;
