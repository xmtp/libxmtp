//! Group members and additions by handle.

use crate::{
    inbox_handles::IdentityTarget,
    tester,
    utils::test::inbox_handles::{random_handle, wait_until_claim_visible},
};
use std::collections::HashSet;
#[cfg(not(target_arch = "wasm32"))]
use {
    crate::context::CoreContext,
    crate::utils::test::{backend::EphemeralBackend, inbox_handles::PAST_CLAIM_INTERVAL},
    xmtp_db::prelude::*,
    xmtp_mls_validation::commit::extract_group_membership,
};

#[cfg(not(target_arch = "wasm32"))]
// verifies: IDENT-086
#[xmtp_common::test(unwrap_try = true)]
async fn members_show_newest_handle_and_other_handles() {
    // Bo claims more than once, so the claims stay off the shared backend.
    let backend = EphemeralBackend::start("").await?;
    tester!(alix, backend: &backend, disable_workers);
    tester!(bo, backend: &backend, disable_workers);
    let group = alix.create_group(None, None)?;
    group.add_members(&[bo.inbox_id()]).await?;

    let (first, second, third) = (random_handle(), random_handle(), random_handle());
    bo.claim_handle(&first).await?;
    backend
        .age_identity_history(bo.inbox_id(), PAST_CLAIM_INTERVAL)
        .await?;
    bo.claim_handle(&second).await?;
    backend
        .age_identity_history(bo.inbox_id(), PAST_CLAIM_INTERVAL)
        .await?;
    bo.claim_handle(&third).await?;
    alix.inbox_addresses(true, vec![bo.inbox_id()]).await?;
    let newest_sequence =
        alix.context.db().get_latest_sequence_id(&[bo.inbox_id()])?[bo.inbox_id()];
    let membership = group
        .with_group_snapshot(|mls_group| Ok(extract_group_membership(mls_group.extensions())?))?;
    assert!(
        (membership.members[bo.inbox_id()] as i64) < newest_sequence,
        "the group still names a state from before the claims"
    );

    alix.clear_stats();
    let members = group.members().await?;
    assert_eq!(
        alix.api_stats().query.get_count(),
        0,
        "members is a local read"
    );

    let bo_member = members
        .iter()
        .find(|member| member.inbox_id == bo.inbox_id())
        .expect("bo is a member");
    assert_eq!(bo_member.handle, Some(third));
    assert_eq!(bo_member.other_handles, vec![second, first]);
    let alix_member = members
        .iter()
        .find(|member| member.inbox_id == alix.inbox_id())
        .expect("alix is a member");
    assert_eq!(alix_member.handle, None);
    assert!(alix_member.other_handles.is_empty());
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
