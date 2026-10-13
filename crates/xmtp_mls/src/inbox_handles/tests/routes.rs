//! Routes that take accounts or handles.

use super::{random_handle, scripted_client, wait_until_claim_visible};
use crate::{
    client::ClientError,
    groups::GroupError,
    inbox_handles::{HandleError, IdentityTarget},
    tester,
};
use std::collections::HashSet;
use xmtp_id::associations::Identifier;
use xmtp_mls_common::group::DMMetadataOptions;

/// The inbox ids of a group's members.
async fn member_ids<Context: crate::context::XmtpSharedContext>(
    group: &crate::groups::MlsGroup<Context>,
) -> Result<HashSet<String>, GroupError> {
    Ok(group
        .members()
        .await?
        .into_iter()
        .map(|member| member.inbox_id)
        .collect())
}

fn no_inbox(error: &ClientError) -> bool {
    matches!(error, ClientError::Handle(HandleError::NotFound))
}

// verifies: IDENT-085
#[xmtp_common::test(unwrap_try = true)]
async fn every_handle_route_rejects_an_unclaimed_answer() {
    tester!(alix, disable_workers);
    tester!(bo);
    // A route that trusted the answer would find this DM, or change this group.
    alix.find_or_create_dm(bo.inbox_id(), None).await?;
    let group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;
    let (client, script) = scripted_client(&alix).await?;
    let handle = random_handle();
    script
        .lock()
        .lies
        .insert(handle.to_string(), bo.inbox_id().to_owned());
    let target = IdentityTarget::Handle(handle);
    let targets = std::slice::from_ref(&target);

    assert!(no_inbox(
        &client.resolve_targets(targets).await.unwrap_err()
    ));
    assert!(client.find_dm_by_target(&target).await?.is_none());
    assert!(no_inbox(
        &client
            .find_or_create_dm_by_target(&target, DMMetadataOptions::default())
            .await
            .unwrap_err()
    ));
    assert!(no_inbox(
        &client
            .create_group_with_targets(targets, None, None)
            .await
            .unwrap_err()
    ));
    let group = client.group(&group.group_id)?;
    assert!(matches!(
        group.add_members_by_targets(targets).await,
        Err(GroupError::Client(ref error)) if no_inbox(error)
    ));
    assert!(matches!(
        group.remove_members_by_targets(targets).await,
        Err(GroupError::Client(ref error)) if no_inbox(error)
    ));
    assert_eq!(script.lock().publishes, 0, "nothing is published");
}

#[xmtp_common::test(unwrap_try = true)]
async fn mixed_targets_make_one_commit() {
    tester!(alix);
    tester!(bo);
    tester!(caro);
    let handle = random_handle();
    bo.claim_handle(&handle).await?;
    wait_until_claim_visible(&alix, &handle).await;
    let group = alix.create_group(None, None)?;
    let epoch = group.epoch().await?;

    group
        .add_members_by_targets(&[
            IdentityTarget::Handle(handle),
            IdentityTarget::Account(caro.identifier()),
        ])
        .await?;

    assert_eq!(group.epoch().await?, epoch + 1);
    let members: HashSet<_> = group
        .members()
        .await?
        .into_iter()
        .map(|member| member.inbox_id)
        .collect();
    assert!(members.contains(bo.inbox_id()));
    assert!(members.contains(caro.inbox_id()));
}

#[xmtp_common::test(unwrap_try = true)]
async fn handle_routes_reach_the_claimed_inbox() {
    tester!(alix);
    tester!(bo);
    tester!(caro);
    let handle = random_handle();
    bo.claim_handle(&handle).await?;
    wait_until_claim_visible(&alix, &handle).await;
    let bo_target = IdentityTarget::Handle(handle);
    let caro_target = IdentityTarget::Account(caro.identifier());

    // The DM by handle is the DM by inbox id.
    let dm = alix
        .find_or_create_dm_by_target(&bo_target, DMMetadataOptions::default())
        .await?;
    let by_inbox = alix.find_or_create_dm(bo.inbox_id(), None).await?;
    assert_eq!(dm.group_id, by_inbox.group_id);
    let found = alix.find_dm_by_target(&bo_target).await?.expect("the DM");
    assert_eq!(found.group_id, dm.group_id);

    // A mixed list creates the group, and later leaves it, in one commit each.
    let group = alix
        .create_group_with_targets(&[bo_target.clone(), caro_target.clone()], None, None)
        .await?;
    assert_eq!(group.epoch().await?, 1);
    let members = member_ids(&group).await?;
    assert!(members.contains(bo.inbox_id()) && members.contains(caro.inbox_id()));
    group
        .remove_members_by_targets(&[bo_target, caro_target])
        .await?;
    assert_eq!(group.epoch().await?, 2);
    assert_eq!(
        member_ids(&group).await?,
        HashSet::from([alix.inbox_id().to_owned()])
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn account_targets_keep_the_account_path() {
    tester!(alix);
    tester!(bo);
    tester!(caro);
    let bo_target = IdentityTarget::Account(bo.identifier());
    let caro_target = IdentityTarget::Account(caro.identifier());

    let dm = alix
        .find_or_create_dm_by_target(&bo_target, DMMetadataOptions::default())
        .await?;
    let found = alix.find_dm_by_target(&bo_target).await?.expect("the DM");
    assert_eq!(found.group_id, dm.group_id);

    let group = alix
        .create_group_with_targets(std::slice::from_ref(&bo_target), None, None)
        .await?;
    group
        .add_members_by_targets(std::slice::from_ref(&caro_target))
        .await?;
    assert!(member_ids(&group).await?.contains(caro.inbox_id()));
    group
        .remove_members_by_targets(std::slice::from_ref(&caro_target))
        .await?;
    assert!(!member_ids(&group).await?.contains(caro.inbox_id()));

    // An account with no inbox keeps the account path's error.
    let unknown = IdentityTarget::Account(Identifier::rand_ethereum());
    assert!(matches!(
        group.add_members_by_targets(&[unknown]).await,
        Err(GroupError::AddressNotFound(_))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn mixed_targets_treat_unknown_accounts_as_the_account_path_does() {
    tester!(alix);
    tester!(bo);
    tester!(caro);
    let handle = random_handle();
    bo.claim_handle(&handle).await?;
    wait_until_claim_visible(&alix, &handle).await;
    let bo_target = IdentityTarget::Handle(handle);
    let unknown = IdentityTarget::Account(Identifier::rand_ethereum());
    let address_not_found = |error: &ClientError| matches!(error, ClientError::Group(group) if matches!(**group, GroupError::AddressNotFound(_)));

    // Create: the account-only error, with or without a handle in the list.
    let account_only = alix
        .create_group_with_targets(std::slice::from_ref(&unknown), None, None)
        .await;
    assert!(address_not_found(&account_only.unwrap_err()));
    let mixed = alix
        .create_group_with_targets(&[bo_target.clone(), unknown.clone()], None, None)
        .await;
    assert!(address_not_found(&mixed.unwrap_err()));

    // Add: the account-only error, and no commit.
    let group = alix
        .create_group_with_targets(&[IdentityTarget::Account(caro.identifier())], None, None)
        .await?;
    let epoch = group.epoch().await?;
    for targets in [
        vec![unknown.clone()],
        vec![bo_target.clone(), unknown.clone()],
    ] {
        assert!(matches!(
            group.add_members_by_targets(&targets).await,
            Err(GroupError::AddressNotFound(_))
        ));
    }
    assert_eq!(group.epoch().await?, epoch);

    // Remove: the unknown account is skipped, with or without a handle.
    group
        .remove_members_by_targets(std::slice::from_ref(&unknown))
        .await?;
    group
        .add_members_by_targets(std::slice::from_ref(&bo_target))
        .await?;
    group
        .remove_members_by_targets(&[bo_target, unknown])
        .await?;
    assert!(!member_ids(&group).await?.contains(bo.inbox_id()));
}
