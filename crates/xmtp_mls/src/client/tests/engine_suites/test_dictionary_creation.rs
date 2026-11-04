//! The leaves of `groups/tests/test_dictionary_creation.rs` that need the client.

use crate::{
    context::XmtpSharedContext,
    groups::{GroupError, MlsGroup, test_support::load_component_registry},
    tester,
};
use openmls::{extensions::ExtensionType, messages::proposals::ProposalType};
use std::collections::BTreeSet;
use xmtp_configuration::{
    ApplicationComponentDefinition, ComponentPermissions, MetadataPolicy,
    PROPOSALS_MIN_PROTOCOL_VERSION,
};
use xmtp_mls_group_state::app_data::{
    component_id::ComponentId, component_registry::ComponentRegistry,
    component_source::read_from_app_data_dict, creation::catalogue_registry_entries,
    protocol_floor::committed_floor_exceeding,
};
use xmtp_proto::{
    types::ConversationType,
    xmtp::mls::message_contents::{
        ComponentType,
        metadata_policy::{Kind, MetadataBasePolicy},
    },
};

const GROUP_FIELD: u16 = 0xC001;

const DM_FIELD: u16 = 0xC002;

const SHARED_FIELD: u16 = 0xC003;

/// A catalogue definition of `component_type` whose three policies are all
/// `base`.
pub(super) fn definition(
    component_id: u16,
    component_type: ComponentType,
    base: MetadataBasePolicy,
    in_groups: bool,
    in_dms: bool,
) -> ApplicationComponentDefinition {
    let policy = Some(MetadataPolicy::Base(base as i32));
    ApplicationComponentDefinition {
        component_id,
        name: format!("app.{component_id:x}"),
        component_type: component_type as i32,
        permissions: ComponentPermissions {
            insert: policy.clone(),
            update: policy.clone(),
            delete: policy,
        },
        in_groups,
        in_dms,
    }
}

/// The application-range IDs a group's registry holds.
fn application_ids<C: XmtpSharedContext>(group: &MlsGroup<C>) -> Result<BTreeSet<u16>, GroupError> {
    Ok(registry(group)?
        .iter()
        .flatten()
        .map(|(id, _)| id.as_u16())
        .filter(|id| *id >= 0xC000)
        .collect())
}

/// The three application fields: one per conversation kind and one for both.
fn catalogue() -> Vec<ApplicationComponentDefinition> {
    let field = |id, in_groups, in_dms| {
        definition(
            id,
            ComponentType::Bytes,
            MetadataBasePolicy::Allow,
            in_groups,
            in_dms,
        )
    };
    vec![
        field(GROUP_FIELD, true, false),
        field(DM_FIELD, false, true),
        field(SHARED_FIELD, true, true),
    ]
}

pub(super) fn registry<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
) -> Result<ComponentRegistry, GroupError> {
    group.with_group_snapshot(|group| Ok(load_component_registry(group)?))
}

pub(super) fn value<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    id: ComponentId,
) -> Result<Option<Vec<u8>>, GroupError> {
    group.with_group_snapshot(|group| Ok(read_from_app_data_dict(id, group).map(<[u8]>::to_vec)))
}

// verifies: META-002
#[xmtp_common::test(unwrap_try = true)]
async fn test_group_context_shape_at_creation() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let group = alix.create_group(None, None)?;
    let dm = crate::groups::MlsGroup::create_dm_and_insert(
        &alix.context,
        xmtp_db::group::GroupMembershipState::Allowed,
        bo.inbox_id().to_string(),
        Default::default(),
        None,
    )?;
    for conversation in [&group, &dm] {
        conversation.with_group_snapshot(|mls_group| {
            let extensions = mls_group.extensions();
            let mut actual: Vec<_> = extensions.iter().map(|ext| ext.extension_type()).collect();
            actual.sort();
            let mut expected = vec![
                ExtensionType::AppDataDictionary,
                ExtensionType::RequiredCapabilities,
            ];
            expected.sort();
            assert_eq!(actual, expected);
            let required = extensions.required_capabilities().unwrap();
            let mut actual = required.extension_types().to_vec();
            actual.sort();
            let mut expected = vec![
                ExtensionType::AppDataDictionary,
                ExtensionType::ApplicationId,
            ];
            expected.sort();
            assert_eq!(actual, expected);
            assert_eq!(required.proposal_types(), &[ProposalType::AppDataUpdate]);
            // META-002 names the codepoint: `app_data_update` is 0x0008.
            assert_eq!(u16::from(ProposalType::AppDataUpdate), 0x0008);
            let dictionary = extensions.app_data_dictionary().unwrap().dictionary();
            assert_eq!(
                dictionary.get(&ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION.as_u16()),
                Some(PROPOSALS_MIN_PROTOCOL_VERSION.as_bytes()),
            );
            assert_eq!(
                committed_floor_exceeding(
                    mls_group,
                    &xmtp_mls_group_state::libxmtp_version::LibXMTPVersion::parse(env!(
                        "CARGO_PKG_VERSION"
                    ))?,
                ),
                None,
            );
            assert_eq!(
                committed_floor_exceeding(
                    mls_group,
                    &xmtp_mls_group_state::libxmtp_version::LibXMTPVersion::parse("999.0.0")?,
                ),
                None,
            );
            Ok(())
        })?;
        assert!(conversation.paused_for_version()?.is_none());
    }
}

/// A new group or DM registers the creator's catalogue entries for its kind
/// with their own type and policies, plus `USER_DISPLAY_NAME` and, in a
/// group, `GROUP_IMAGE`, and writes none of their values. The registry is
/// read before any commit, so reconciliation cannot supply it. A joiner
/// without the catalogue reads the same registry from the group.
// verifies: META-066, PERM-029
#[xmtp_common::test(unwrap_try = true)]
async fn test_creation_registers_configured_fields() {
    tester!(alix, configured: |c| c.application_components = catalogue());
    tester!(bo);
    let group = alix.create_group(None, None)?;
    let dm = MlsGroup::create_dm_and_insert(
        &alix.context,
        xmtp_db::group::GroupMembershipState::Allowed,
        bo.inbox_id().to_string(),
        Default::default(),
        None,
    )?;

    for (created, kind, fields, image) in [
        (
            &group,
            ConversationType::Group,
            [GROUP_FIELD, SHARED_FIELD],
            true,
        ),
        (&dm, ConversationType::Dm, [DM_FIELD, SHARED_FIELD], false),
    ] {
        assert_eq!(application_ids(created)?, BTreeSet::from(fields));
        let registry = registry(created)?;
        for (id, metadata) in catalogue_registry_entries(&catalogue(), kind) {
            assert_eq!(registry.get(&id)?, Some(metadata));
        }
        let permissions = registry
            .get(&ComponentId::USER_DISPLAY_NAME)??
            .permissions?;
        let self_owned = Some(Kind::Base(
            MetadataBasePolicy::AllowIfSelfOrNonMember as i32,
        ));
        for policy in [
            permissions.insert_policy,
            permissions.update_policy,
            permissions.delete_policy,
        ] {
            assert_eq!(policy?.kind, self_owned);
        }
        let image_entry = registry.get(&ComponentId::GROUP_IMAGE)?;
        assert_eq!(image_entry.is_some(), image);
        if let Some(image_entry) = image_entry {
            assert_eq!(
                image_entry.permissions,
                registry.get(&ComponentId::GROUP_IMAGE_URL)??.permissions,
            );
        }
        for id in fields
            .map(ComponentId::new)
            .into_iter()
            .chain([ComponentId::USER_DISPLAY_NAME, ComponentId::GROUP_IMAGE])
        {
            assert_eq!(value(created, id)?, None);
        }
    }

    let registry = value(&group, ComponentId::COMPONENT_REGISTRY)?;
    group.add_members(&[bo.inbox_id()]).await?;
    let joined = bo.wait_for_welcomes().await?.pop()?;
    assert_eq!(value(&joined, ComponentId::COMPONENT_REGISTRY)?, registry);
}
