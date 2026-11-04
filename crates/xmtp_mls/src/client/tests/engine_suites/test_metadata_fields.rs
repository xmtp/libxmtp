//! The leaves of `groups/tests/test_metadata_fields.rs` that need the client.

use super::test_dictionary_creation::definition;
use crate::{
    groups::{GroupError, MlsGroup},
    tester,
};
use xmtp_configuration::ApplicationComponentDefinition;
use xmtp_mls_group_state::app_data::{
    component_id::ComponentId,
    fields::{MetadataComponentType, MetadataFieldRef, MetadataKeyType, MetadataScalarType},
};
use xmtp_proto::xmtp::mls::message_contents::{ComponentType, metadata_policy::MetadataBasePolicy};

/// A string field in groups and DMs.
const STATUS: u16 = 0xC001;

/// A per-user string field in groups only.
const NICKNAME: u16 = 0xC002;

/// A string field only admins may write.
const TOPIC: u16 = 0xC003;

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
