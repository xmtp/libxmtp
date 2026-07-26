//! Group creation writes `sequence_id: 0` for the creator. A commit before the
//! first `UpdateGroupMembership` must still validate. For the opposite case, a
//! commit that chooses `0`, see
//! `identity_updates::tests::get_installation_diff_rejects_added_inbox_at_sequence_zero`.

use crate::tester;

/// The GCE proposal carries `{alix: 0}` directly.
#[xmtp_common::test(unwrap_try = true)]
async fn metadata_update_on_freshly_created_group_succeeds() {
    tester!(alix);

    let group = alix.create_group(None, None).await?;
    group.update_group_name("hello".to_string()).await?;

    assert_eq!(group.group_name().await?, "hello");
}

/// The commit has no GCE proposal. `get_latest_group_membership` then reads
/// the staged commit group context. That context still holds `{alix: 0}`.
#[xmtp_common::test(unwrap_try = true)]
async fn key_update_on_freshly_created_group_succeeds() {
    tester!(alix);

    let group = alix.create_group(None, None).await?;

    group.key_update().await?;
}
