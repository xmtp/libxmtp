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
