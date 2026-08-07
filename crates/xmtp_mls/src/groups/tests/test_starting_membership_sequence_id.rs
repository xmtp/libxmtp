//! Regression coverage for the `sequence_id: 0` placeholder that
//! `build_starting_group_membership_extension` writes at group creation.
//!
//! `create_group` is purely local and seeds the membership extension with the
//! creator at sequence_id `0`. Nothing replaces it with the creator's real
//! sequence id until an `UpdateGroupMembership` intent runs, and neither the
//! metadata-update nor the key-update path triggers one.
//!
//! `0` is a sentinel meaning "unset" — `get_installation_diff` honors it on the
//! *old* membership side (`Some(0) => None`), but the reads on the *new* side
//! treat it as a literal sequence id. The actor credential check in
//! `ValidatedCommit::from_staged_commit` then resolves the committer's
//! association state at sequence id `0`, and since no identity update can have
//! `sequence_id <= 0`, it fails with `MissingIdentityUpdate` — which is not
//! retryable, so the intent lands in `Error` permanently.

use crate::tester;

/// A metadata update issued before anything has bumped the creator's
/// placeholder sequence id must still commit successfully.
#[xmtp_common::test(unwrap_try = true)]
async fn metadata_update_on_freshly_created_group_succeeds() {
    tester!(alix);

    // Membership is `{alix: 0}` at this point — creation does no network I/O
    // and queues no membership-update intent.
    let group = alix.create_group(None, None)?;

    // This commit's GroupContextExtensions proposal carries `{alix: 0}`.
    group.update_group_name("hello".to_string()).await?;

    assert_eq!(group.group_name()?, "hello");
}

/// Same placeholder, reached through a commit with no GCE proposal at all —
/// `get_latest_group_membership` falls back to the staged commit's group
/// context, which still holds `{alix: 0}`.
#[xmtp_common::test(unwrap_try = true)]
async fn key_update_on_freshly_created_group_succeeds() {
    tester!(alix);

    let group = alix.create_group(None, None)?;

    group.key_update().await?;
}
