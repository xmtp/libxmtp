//! Proposal permission tests: registry actions.

use super::*;

/// A raw registry mutation must not relax the constrained `ADMIN_LIST` entry.
/// The shared constrained-component check rejects it before publication.
// verifies: PERM-017
#[xmtp_common::test(unwrap_try = true)]
async fn test_raw_registry_admin_list_allow_rejected_before_publish() {
    use crate::groups::{
        app_data::load_component_registry,
        intents::{AppDataUpdateIntentData, QueueIntent},
    };
    use prost::Message as _;
    use tls_codec::VLBytes;
    use xmtp_mls_group_state::app_data::{
        component_id::ComponentId, components::tls_map_components::ComponentRegistryComponent,
        typed::Component,
    };
    use xmtp_mls_tls_collections::tls_map::TlsMapDelta;
    use xmtp_proto::xmtp::mls::message_contents::{
        MetadataPolicy,
        metadata_policy::{Kind as MetadataPolicyKind, MetadataBasePolicy},
    };

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.first()?.clone();
    bo_group.sync().await?;

    let before_permissions = bo_group.permissions()?;
    let before_dictionary = bo_group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<_, crate::groups::GroupError>(
                mls_group
                    .extensions()
                    .app_data_dictionary()
                    .expect("app-data dictionary is present after bootstrap")
                    .dictionary()
                    .clone(),
            )
        })
        .await?;
    let payload = alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            let registry = load_component_registry(&mls_group)?;
            let mut metadata = registry
                .get(&ComponentId::ADMIN_LIST)?
                .expect("ADMIN_LIST is registered after bootstrap");
            metadata
                .permissions
                .as_mut()
                .expect("ADMIN_LIST has permissions")
                .insert_policy = Some(MetadataPolicy {
                kind: Some(MetadataPolicyKind::Base(MetadataBasePolicy::Allow as i32)),
            });
            let delta = TlsMapDelta::new().update(
                ComponentId::ADMIN_LIST,
                VLBytes::new(metadata.encode_to_vec()),
            );
            <ComponentRegistryComponent as Component>::encode_mutation(&delta)
                .map_err(|error| crate::groups::GroupError::ComponentSource(error.into()))
        })
        .await?;
    let intent = QueueIntent::app_data_update()
        .data(Vec::<u8>::from(AppDataUpdateIntentData::new(
            ComponentId::COMPONENT_REGISTRY.as_u16(),
            payload,
        )))
        .queue(&alix_group)?;

    assert!(
        alix_group
            .sync_until_intent_resolved(intent.id)
            .await
            .is_err(),
        "a raw ADMIN_LIST Allow policy must be rejected"
    );
    bo_group.sync().await?;
    let topic = xmtp_db::incoming_envelope::StreamTopic::group(bo_group.group_id);
    // The constrained-component check rejects this payload before publication.
    assert!(bo.context.db().read_last_rejection(&topic)?.is_none());
    assert_eq!(bo_group.permissions()?, before_permissions);
    let after_dictionary = bo_group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<_, crate::groups::GroupError>(
                mls_group
                    .extensions()
                    .app_data_dictionary()
                    .expect("app-data dictionary is present after rejection")
                    .dictionary()
                    .clone(),
            )
        })
        .await?;
    assert_eq!(after_dictionary, before_dictionary);
}

/// A valid registry update alone changes the public action policy view.
#[xmtp_common::test(unwrap_try = true)]
async fn test_registry_alone_controls_action_policy_view() {
    use crate::groups::{
        app_data::load_component_registry,
        intents::{AppDataUpdateIntentData, QueueIntent},
    };
    use prost::Message as _;
    use tls_codec::VLBytes;
    use xmtp_mls_group_state::app_data::{
        component_id::ComponentId, components::tls_map_components::ComponentRegistryComponent,
        typed::Component,
    };
    use xmtp_mls_tls_collections::tls_map::TlsMapDelta;
    use xmtp_proto::xmtp::mls::message_contents::{
        MetadataPolicy,
        metadata_policy::{Kind as MetadataPolicyKind, MetadataBasePolicy},
    };

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_group = bo.wait_for_welcomes().await?.first()?.clone();
    bo_group.sync().await?;

    let payload = alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            let registry = load_component_registry(&mls_group)?;
            let mut metadata = registry
                .get(&ComponentId::ADMIN_LIST)?
                .expect("ADMIN_LIST is registered after bootstrap");
            metadata
                .permissions
                .as_mut()
                .expect("ADMIN_LIST has permissions")
                .insert_policy = Some(MetadataPolicy {
                kind: Some(MetadataPolicyKind::Base(
                    MetadataBasePolicy::AllowIfAdmin as i32,
                )),
            });
            let delta = TlsMapDelta::new().update(
                ComponentId::ADMIN_LIST,
                VLBytes::new(metadata.encode_to_vec()),
            );
            <ComponentRegistryComponent as Component>::encode_mutation(&delta)
                .map_err(|error| crate::groups::GroupError::ComponentSource(error.into()))
        })
        .await?;
    let intent = QueueIntent::app_data_update()
        .data(Vec::<u8>::from(AppDataUpdateIntentData::new(
            ComponentId::COMPONENT_REGISTRY.as_u16(),
            payload,
        )))
        .queue(&alix_group)?;

    alix_group.sync_until_intent_resolved(intent.id).await?;
    bo_group.sync().await?;
    for group in [&alix_group, &bo_group] {
        assert_eq!(
            group.permissions()?.policies.add_admin_policy,
            crate::groups::group_permissions::PermissionsPolicies::allow_if_actor_admin(),
        );
    }
}

/// Check all four registry action slots and the public view on both peers.
#[xmtp_common::test(unwrap_try = true)]
async fn test_migrated_action_permission_updates_use_registry() {
    use crate::groups::{
        group_permissions::{MembershipPolicies, PermissionsPolicies},
        intents::{PermissionPolicyOption, PermissionUpdateType},
    };
    use xmtp_mls_group_state::app_data::component_id::ComponentId;
    use xmtp_proto::xmtp::mls::message_contents::metadata_policy::{
        Kind as MetadataPolicyKind, MetadataBasePolicy,
    };

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let bo_groups = bo.wait_for_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    bo_group.sync().await?;

    let mut expected = alix_group.permissions()?.policies;
    let updates = [
        (
            PermissionUpdateType::AddMember,
            PermissionPolicyOption::Deny,
        ),
        (
            PermissionUpdateType::RemoveMember,
            PermissionPolicyOption::Deny,
        ),
        (
            PermissionUpdateType::AddAdmin,
            PermissionPolicyOption::AdminOnly,
        ),
        (
            PermissionUpdateType::RemoveAdmin,
            PermissionPolicyOption::Deny,
        ),
    ];

    for (update_type, policy_option) in updates {
        let epoch = alix_group.epoch().await?;
        alix_group
            .update_permission_policy(update_type.clone(), policy_option.clone(), None)
            .await?;
        bo_group.sync().await?;
        assert_eq!(
            alix_group.epoch().await?,
            epoch + 1,
            "{update_type:?} did not commit the registry update"
        );
        assert_eq!(
            bo_group.epoch().await?,
            epoch + 1,
            "receiver did not apply the registry update"
        );

        match &update_type {
            PermissionUpdateType::AddMember => {
                expected.add_member_policy = MembershipPolicies::from(policy_option.clone());
            }
            PermissionUpdateType::RemoveMember => {
                expected.remove_member_policy = MembershipPolicies::from(policy_option.clone());
            }
            PermissionUpdateType::AddAdmin => {
                expected.add_admin_policy = PermissionsPolicies::from(policy_option.clone());
            }
            PermissionUpdateType::RemoveAdmin => {
                expected.remove_admin_policy = PermissionsPolicies::from(policy_option.clone());
            }
            PermissionUpdateType::UpdateMetadata => unreachable!(),
        }

        for (label, group) in [("alix", &alix_group), ("bo", bo_group)] {
            assert_eq!(
                group.permissions()?.policies,
                expected,
                "{label} did not project the registry action policy"
            );

            let (component_id, policy) = match &update_type {
                PermissionUpdateType::AddMember => (ComponentId::GROUP_MEMBERSHIP, "insert"),
                PermissionUpdateType::RemoveMember => (ComponentId::GROUP_MEMBERSHIP, "delete"),
                PermissionUpdateType::AddAdmin => (ComponentId::ADMIN_LIST, "insert"),
                PermissionUpdateType::RemoveAdmin => (ComponentId::ADMIN_LIST, "delete"),
                PermissionUpdateType::UpdateMetadata => unreachable!(),
            };
            let expected_base = match &policy_option {
                PermissionPolicyOption::Allow => MetadataBasePolicy::Allow,
                PermissionPolicyOption::Deny => MetadataBasePolicy::Deny,
                PermissionPolicyOption::AdminOnly => MetadataBasePolicy::AllowIfAdmin,
                PermissionPolicyOption::SuperAdminOnly => MetadataBasePolicy::AllowIfSuperAdmin,
            } as i32;
            let registry = group
                .load_mls_group_with_lock_async(async |mls_group| {
                    Ok::<_, crate::groups::GroupError>(
                        crate::groups::app_data::load_component_registry(&mls_group)?,
                    )
                })
                .await?;
            let metadata = registry
                .get(&component_id)?
                .unwrap_or_else(|| panic!("{label} registry missing {component_id:?}"));
            let permissions = metadata
                .permissions
                .as_ref()
                .unwrap_or_else(|| panic!("{label} registry entry missing permissions"));
            let stored_policy = match policy {
                "insert" => permissions.insert_policy.as_ref(),
                "delete" => permissions.delete_policy.as_ref(),
                _ => unreachable!(),
            }
            .and_then(|policy| policy.kind.as_ref())
            .unwrap_or_else(|| panic!("{label} registry entry missing {policy} policy"));
            assert!(
                matches!(stored_policy, MetadataPolicyKind::Base(base) if *base == expected_base),
                "{label} registry {policy} policy does not match the action policy: {stored_policy:?}"
            );
        }
    }
}

/// `Allow` would give every member authority to change the admin list. Reject
/// it before queueing an intent or advancing the group epoch.
// verifies: PERM-018
#[xmtp_common::test(unwrap_try = true)]
async fn test_allow_admin_permission_updates_rejected_before_queueing() {
    use crate::groups::{
        GroupError,
        error::MetadataPermissionsError,
        intents::{PermissionPolicyOption, PermissionUpdateType},
    };

    let alix = EngineTestPeer::new().await;

    let group = alix.create_group(None, None)?;
    let epoch = group.epoch().await?;
    let intents = group.context.db().find_group_intents(
        group.group_id,
        None,
        Some(vec![IntentKind::UpdatePermission]),
    )?;

    for update_type in [
        PermissionUpdateType::AddAdmin,
        PermissionUpdateType::RemoveAdmin,
    ] {
        let error = group
            .update_permission_policy(update_type.clone(), PermissionPolicyOption::Allow, None)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            GroupError::MetadataPermissionsError(MetadataPermissionsError::InvalidPermissionUpdate)
        ));
        assert_eq!(
            group.epoch().await?,
            epoch,
            "rejected update advanced epoch"
        );
        assert_eq!(
            group
                .context
                .db()
                .find_group_intents(
                    group.group_id,
                    None,
                    Some(vec![IntentKind::UpdatePermission]),
                )?
                .len(),
            intents.len(),
            "rejected {update_type:?} update queued an intent"
        );
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_receiver_rejects_invalid_registry_action_state() {
    use crate::groups::{
        app_data::load_component_registry,
        intents::{AppDataUpdateIntentData, QueueIntent},
    };
    use prost::Message;
    use tls_codec::VLBytes;
    use xmtp_mls_group_state::app_data::{
        component_id::ComponentId, components::tls_map_components::ComponentRegistryComponent,
        typed::Component,
    };
    use xmtp_mls_tls_collections::tls_map::TlsMapDelta;
    use xmtp_proto::xmtp::mls::message_contents::{
        MetadataPolicy,
        metadata_policy::{AnyCondition, Kind, MetadataBasePolicy},
    };

    for (id, malformed_tree) in [
        (ComponentId::GROUP_MEMBERSHIP, false),
        (ComponentId::ADMIN_LIST, false),
        (ComponentId::GROUP_MEMBERSHIP, true),
    ] {
        let alix = EngineTestPeer::new().await;
        let bo = EngineTestPeer::new().await;
        let group = alix
            .create_group_with_members(&[bo.inbox_id()], None, None)
            .await?;
        let received = bo.wait_for_welcomes().await?;
        let peer = received.first()?;
        peer.sync().await?;
        let before = peer.permissions()?;
        let epoch = peer.epoch().await?;
        let payload = group.with_group_snapshot(|group| {
            let delta = if malformed_tree {
                let mut metadata = load_component_registry(group)?.get(&id).unwrap().unwrap();
                metadata.permissions.as_mut().unwrap().insert_policy = Some(MetadataPolicy {
                    kind: Some(Kind::AnyCondition(AnyCondition {
                        policies: vec![
                            MetadataPolicy {
                                kind: Some(Kind::Base(MetadataBasePolicy::Allow as i32)),
                            },
                            MetadataPolicy {
                                kind: Some(Kind::Base(i32::MAX)),
                            },
                        ],
                    })),
                });
                TlsMapDelta::new().update(id, VLBytes::new(metadata.encode_to_vec()))
            } else {
                TlsMapDelta::<ComponentId, VLBytes>::new().delete(id)
            };
            <ComponentRegistryComponent as Component>::encode_mutation(&delta)
                .map_err(|error| crate::groups::GroupError::ComponentSource(error.into()))
        })?;
        let intent = QueueIntent::app_data_update()
            .data(Vec::<u8>::from(AppDataUpdateIntentData::new(
                ComponentId::COMPONENT_REGISTRY.as_u16(),
                payload,
            )))
            .queue(&group)?;
        assert!(group.sync_until_intent_resolved(intent.id).await.is_err());
        let _ = peer.sync().await;
        let topic = xmtp_db::incoming_envelope::StreamTopic::group(peer.group_id);
        let rejection = bo
            .context
            .db()
            .read_last_rejection(&topic)?
            .expect("peer must reject invalid action state");
        assert_eq!(
            bo.context.db().topic_progress(&topic)?.processed,
            rejection.sequence_id
        );
        assert_eq!(peer.epoch().await?, epoch);
        assert_eq!(peer.permissions()?, before);
    }
}
