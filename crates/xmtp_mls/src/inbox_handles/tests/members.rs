//! Group members carry the handles of the newest association state held.

use super::{ClaimSigner::Installation, PAST_CLAIM_INTERVAL, claim_for_test, random_handle};
use crate::{context::XmtpSharedContext, tester, utils::test::backend::EphemeralBackend};
use xmtp_db::prelude::*;
use xmtp_mls_validation::commit::extract_group_membership;

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
    claim_for_test(&bo, &first, Installation).await?;
    backend
        .age_identity_history(bo.inbox_id(), PAST_CLAIM_INTERVAL)
        .await?;
    claim_for_test(&bo, &second, Installation).await?;
    backend
        .age_identity_history(bo.inbox_id(), PAST_CLAIM_INTERVAL)
        .await?;
    claim_for_test(&bo, &third, Installation).await?;
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
