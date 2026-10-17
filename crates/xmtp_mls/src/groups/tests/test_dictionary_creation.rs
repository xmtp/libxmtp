//! Dictionary shape at creation, configured field registration, and
//! registry reconciliation by membership commits.

use std::collections::BTreeSet;

use crate::{
    context::XmtpSharedContext,
    groups::{GroupError, MlsGroup, UpdateAdminListType, app_data::load_component_registry},
    tester,
};
use openmls::{extensions::ExtensionType, messages::proposals::ProposalType};
use xmtp_configuration::{
    ApplicationComponentDefinition, ComponentPermissions, MetadataPolicy,
    PROPOSALS_MIN_PROTOCOL_VERSION,
};
use xmtp_db::{incoming_envelope::StreamTopic, prelude::*};
use xmtp_mls_common::app_data::{
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
    group.with_group_snapshot(|group| Ok(read_from_app_data_dict(id, group)))
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
                ExtensionType::LastResort,
                ExtensionType::ApplicationId,
            ];
            expected.sort();
            assert_eq!(actual, expected);
            assert_eq!(required.proposal_types(), &[ProposalType::AppDataUpdate]);
            let dictionary = extensions.app_data_dictionary().unwrap().dictionary();
            assert_eq!(
                dictionary.get(&ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION.as_u16()),
                Some(PROPOSALS_MIN_PROTOCOL_VERSION.as_bytes()),
            );
            assert_eq!(
                committed_floor_exceeding(
                    mls_group,
                    &xmtp_mls_common::libxmtp_version::LibXMTPVersion::parse(env!(
                        "CARGO_PKG_VERSION"
                    ))?,
                ),
                None,
            );
            assert_eq!(
                committed_floor_exceeding(
                    mls_group,
                    &xmtp_mls_common::libxmtp_version::LibXMTPVersion::parse("999.0.0")?,
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

/// A super admin's membership commit registers the catalogue entries the
/// group lacks. A member whose own snapshot has no catalogue accepts it, and
/// so does one whose snapshot defines the same ID differently, because
/// received commits are judged only from group state.
// verifies: META-067
#[xmtp_common::test(unwrap_try = true)]
async fn test_membership_commit_registers_missing_fields() {
    tester!(alix);
    tester!(bo, configured: |c| c.application_components = catalogue());
    tester!(carol);
    tester!(dave, configured: |c| c.application_components = vec![definition(
        GROUP_FIELD,
        ComponentType::String,
        MetadataBasePolicy::AllowIfSuperAdmin,
        true,
        false,
    )]);
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), dave.inbox_id()], None, None)
        .await?;
    assert!(application_ids(&group)?.is_empty());
    group
        .update_admin_list(UpdateAdminListType::AddSuper, bo.inbox_id().to_string())
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.pop()?;
    let dave_group = dave.wait_for_welcomes().await?.pop()?;
    bo_group.sync().await?;
    dave_group.sync().await?;

    bo_group.add_members(&[carol.inbox_id()]).await?;
    group.sync().await?;
    dave_group.sync().await?;
    let carol_group = carol.wait_for_welcomes().await?.pop()?;
    let topic = StreamTopic::group(group.group_id);
    assert!(alix.context.db().read_last_rejection(&topic)?.is_none());
    assert!(dave.context.db().read_last_rejection(&topic)?.is_none());
    assert_eq!(group.epoch().await?, bo_group.epoch().await?);
    assert_eq!(dave_group.epoch().await?, bo_group.epoch().await?);
    for member in [&group, &bo_group, &carol_group, &dave_group] {
        assert_eq!(
            application_ids(member)?,
            BTreeSet::from([GROUP_FIELD, SHARED_FIELD])
        );
    }
    let (_, registered) = catalogue_registry_entries(&catalogue(), ConversationType::Group)
        .find(|(id, _)| id.as_u16() == GROUP_FIELD)?;
    assert_eq!(
        registry(&dave_group)?.get(&ComponentId::new(GROUP_FIELD))?,
        Some(registered)
    );
    assert_eq!(group.members().await?.len(), 4);
}

/// Registration rides on any commit by a client with registry authority,
/// not only a membership change: a super admin's metadata write registers
/// the fields the group lacks.
// verifies: META-067
#[xmtp_common::test(unwrap_try = true)]
async fn test_metadata_write_registers_missing_fields() {
    tester!(alix);
    tester!(bo, configured: |c| c.application_components = catalogue());
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    group
        .update_admin_list(UpdateAdminListType::AddSuper, bo.inbox_id().to_string())
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.pop()?;
    bo_group.sync().await?;
    assert!(application_ids(&bo_group)?.is_empty());

    bo_group.update_group_name("Renamed".into()).await?;
    group.sync().await?;
    assert_eq!(group.epoch().await?, bo_group.epoch().await?);
    for member in [&group, &bo_group] {
        assert_eq!(
            application_ids(member)?,
            BTreeSet::from([GROUP_FIELD, SHARED_FIELD])
        );
    }
}

/// Reconciliation inserts only missing IDs. An entry the group already
/// holds keeps its committed type and policies even when the committer's
/// backend defines that ID differently.
// verifies: META-067
#[xmtp_common::test(unwrap_try = true)]
async fn test_reconciliation_never_overwrites_an_entry() {
    tester!(alix, configured: |c| c.application_components = vec![definition(
        GROUP_FIELD,
        ComponentType::Bytes,
        MetadataBasePolicy::Allow,
        true,
        false,
    )]);
    tester!(bo, configured: |c| c.application_components = vec![
        definition(GROUP_FIELD, ComponentType::String, MetadataBasePolicy::AllowIfSuperAdmin, true, false),
        definition(SHARED_FIELD, ComponentType::Bytes, MetadataBasePolicy::Allow, true, false),
    ]);
    tester!(carol);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let committed = registry(&group)?.get(&ComponentId::new(GROUP_FIELD))??;
    group
        .update_admin_list(UpdateAdminListType::AddSuper, bo.inbox_id().to_string())
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.pop()?;
    bo_group.sync().await?;

    bo_group.add_members(&[carol.inbox_id()]).await?;
    group.sync().await?;
    let registry = registry(&group)?;
    assert_eq!(registry.get(&ComponentId::new(GROUP_FIELD))??, committed);
    assert!(registry.contains(&ComponentId::new(SHARED_FIELD)));
    assert_eq!(group.members().await?.len(), 3);
}

/// A member without registry authority still commits membership changes; it
/// only leaves the missing entries for a super admin to register.
// verifies: META-067
#[xmtp_common::test(unwrap_try = true)]
async fn test_membership_commit_without_authority_skips_registration() {
    tester!(alix);
    tester!(bo, configured: |c| c.application_components = catalogue());
    tester!(carol);
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.pop()?;

    bo_group.add_members(&[carol.inbox_id()]).await?;
    group.sync().await?;
    assert_eq!(group.members().await?.len(), 3);
    assert!(application_ids(&group)?.is_empty());
    assert!(application_ids(&bo_group)?.is_empty());
}

/// In a DM, either participant registers missing application entries under
/// the DM registry exception, here while adding the peer's new installation,
/// and the peer accepts it without a catalogue.
// verifies: META-067
#[xmtp_common::test(unwrap_try = true)]
async fn test_dm_participant_registers_missing_fields() {
    tester!(alix, configured: |c| c.application_components = catalogue());
    tester!(bo);
    let bo_dm = bo.find_or_create_dm(alix.inbox_id(), None).await?;
    let dm = alix.wait_for_welcomes().await?.pop()?;
    assert!(application_ids(&dm)?.is_empty());
    tester!(_bo2, from: bo);

    dm.update_installations().await?;
    bo_dm.sync().await?;
    assert_eq!(bo_dm.epoch().await?, dm.epoch().await?);
    for participant in [&dm, &bo_dm] {
        assert_eq!(
            application_ids(participant)?,
            BTreeSet::from([DM_FIELD, SHARED_FIELD])
        );
    }
}

/// Two super admins that race membership commits register a missing entry
/// once: the commit that loses the epoch is rebuilt, finds the entry, and
/// still lands its membership change.
// verifies: META-067
#[xmtp_common::test(unwrap_try = true)]
async fn test_racing_membership_commits_register_once() {
    tester!(alix);
    tester!(bo, configured: |c| c.application_components = catalogue());
    tester!(carol, configured: |c| c.application_components = catalogue());
    tester!(dave);
    tester!(eve);
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), carol.inbox_id()], None, None)
        .await?;
    for admin in [bo.inbox_id(), carol.inbox_id()] {
        group
            .update_admin_list(UpdateAdminListType::AddSuper, admin.to_string())
            .await?;
    }
    let bo_group = bo.wait_for_welcomes().await?.pop()?;
    let carol_group = carol.wait_for_welcomes().await?.pop()?;
    bo_group.sync().await?;
    carol_group.sync().await?;

    // Carol builds her commit on the epoch Bo's commit has already consumed.
    bo_group.add_members(&[dave.inbox_id()]).await?;
    carol_group.add_members(&[eve.inbox_id()]).await?;
    for member in [&group, &bo_group, &carol_group] {
        member.sync().await?;
        assert_eq!(
            application_ids(member)?,
            BTreeSet::from([GROUP_FIELD, SHARED_FIELD])
        );
        assert_eq!(member.members().await?.len(), 5);
    }
    assert_eq!(group.epoch().await?, carol_group.epoch().await?);
    // Carol's first commit targeted the consumed epoch, so the race happened.
    let topic = StreamTopic::group(group.group_id);
    assert_eq!(
        alix.context.db().read_last_rejection(&topic)??.code,
        "mls_processing_failure"
    );
}
