//! Routes that take accounts or handles.

use super::{random_handle, scripted_client, wait_until_claim_visible};
use crate::{
    client::ClientError,
    groups::GroupError,
    inbox_handles::{HandleError, IdentityTarget},
    tester,
};
use std::collections::HashSet;
use xmtp_mls_common::group::DMMetadataOptions;

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
