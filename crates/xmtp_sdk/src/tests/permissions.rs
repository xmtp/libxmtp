use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn disappearing_permission_denies_both_metadata_fields() {
    use crate::{MetadataFieldKind, PermissionPolicy, PermissionUpdateKind};
    use xmtp_mls::groups::group_permissions::MetadataPolicies;
    use xmtp_mls::mls_common::group_mutable_metadata::MetadataField;

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;

    group
        .update_permission(
            PermissionUpdateKind::UpdateMetadata,
            PermissionPolicy::Deny,
            Some(MetadataFieldKind::Disappearing),
        )
        .await?;

    let snapshot = group.inner.state_snapshot()?;
    let policies = &snapshot
        .group
        .expect("group metadata snapshot")
        .permissions
        .policies;
    let from_ns = policies
        .update_metadata_policy
        .get(MetadataField::MessageDisappearFromNS.as_str())
        .cloned();
    let in_ns = policies
        .update_metadata_policy
        .get(MetadataField::MessageDisappearInNS.as_str())
        .cloned();
    assert_eq!(
        from_ns,
        Some(MetadataPolicies::deny()),
        "MessageDisappearFromNS must deny after the shared disappearing-message policy is denied"
    );
    assert_eq!(
        from_ns, in_ns,
        "MessageDisappearFromNS and MessageDisappearInNS must not diverge"
    );

    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn disappearing_permission_read_reports_divergent_fields() {
    use crate::PermissionPolicy;
    use xmtp_mls::groups::group_permissions::MetadataPolicies;
    use xmtp_mls::mls_common::group_mutable_metadata::MetadataField;

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let mut snapshot = group.inner.state_snapshot()?;
    let policies = &mut snapshot
        .group
        .as_mut()
        .expect("group metadata snapshot")
        .permissions
        .policies;
    policies.update_metadata_policy.insert(
        MetadataField::MessageDisappearFromNS.as_str().into(),
        MetadataPolicies::deny(),
    );
    assert_ne!(
        policies
            .update_metadata_policy
            .get(MetadataField::MessageDisappearFromNS.as_str()),
        policies
            .update_metadata_policy
            .get(MetadataField::MessageDisappearInNS.as_str()),
        "test needs different policies for the two fields"
    );

    let state = crate::GroupState::from_snapshot(snapshot)?;
    assert!(matches!(
        state.permissions.policy_set.update_disappearing,
        PermissionPolicy::Other
    ));
    client.end().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn custom_permission_set_is_converted_and_invalid_set_is_rejected() {
    use crate::{
        CreateGroupOptions, GroupPermissionMode, PermissionPolicy as Policy, PermissionPolicySet,
    };
    let policy_set = PermissionPolicySet {
        add_member: Policy::Allow,
        remove_member: Policy::Deny,
        add_admin: Policy::Admin,
        remove_admin: Policy::Admin,
        update_name: Policy::Admin,
        update_description: Policy::Allow,
        update_image: Policy::Admin,
        update_disappearing: Policy::Admin,
        update_app_data: Policy::SuperAdmin,
    };
    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = alix
        .conversations()
        .create_group(
            vec![],
            Some(CreateGroupOptions {
                permissions: Some(GroupPermissionMode::Custom {
                    policy_set: policy_set.clone(),
                }),
                ..Default::default()
            }),
        )
        .await?;
    let actual = group.state().await?.permissions.policy_set;
    assert!(matches!(actual.add_member, Policy::Allow));
    assert!(matches!(actual.remove_member, Policy::Deny));
    assert!(matches!(actual.add_admin, Policy::Admin));
    assert!(matches!(actual.remove_admin, Policy::Admin));
    assert!(matches!(actual.update_name, Policy::Admin));
    assert!(matches!(actual.update_description, Policy::Allow));
    assert!(matches!(actual.update_image, Policy::Admin));
    assert!(matches!(actual.update_disappearing, Policy::Admin));
    assert!(matches!(actual.update_app_data, Policy::SuperAdmin));

    let invalid = PermissionPolicySet {
        add_admin: Policy::Allow,
        ..policy_set
    };
    assert!(matches!(
        alix.conversations()
            .create_group(
                vec![],
                Some(CreateGroupOptions {
                    permissions: Some(GroupPermissionMode::Custom {
                        policy_set: invalid
                    }),
                    ..Default::default()
                })
            )
            .await,
        Err(XmtpError::InvalidInput(_))
    ));
    alix.end().await?;
}

// Each member reports the permission level of its role, and the level follows
// admin and super admin changes.
#[xmtp_common::test(unwrap_try = true)]
async fn member_permission_levels_follow_role_changes() {
    use crate::PermissionLevel;

    async fn level_of(
        group: &crate::Group,
        inbox_id: &InboxId,
    ) -> Result<PermissionLevel, XmtpError> {
        Ok(group
            .members()
            .await?
            .into_iter()
            .find(|member| member.inbox_id == *inbox_id)
            .expect("member")
            .permission_level)
    }

    let alix = Client::create(crate::generate_local_signer().await, options()).await?;
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = alix
        .conversations()
        .create_group(vec![bo.inbox_id()], None)
        .await?;
    assert!(matches!(
        level_of(&group, &alix.inbox_id()).await?,
        PermissionLevel::SuperAdmin
    ));
    assert!(matches!(
        level_of(&group, &bo.inbox_id()).await?,
        PermissionLevel::Member
    ));

    group.add_admin(bo.inbox_id()).await?;
    assert!(matches!(
        level_of(&group, &bo.inbox_id()).await?,
        PermissionLevel::Admin
    ));

    group.add_super_admin(bo.inbox_id()).await?;
    assert!(matches!(
        level_of(&group, &bo.inbox_id()).await?,
        PermissionLevel::SuperAdmin
    ));

    alix.end().await?;
    bo.end().await?;
}
