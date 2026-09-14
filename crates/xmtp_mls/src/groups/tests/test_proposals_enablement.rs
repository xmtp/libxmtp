//! Enabling proposals and minimum-version gating.

use crate::{groups::EnableProposalsOptions, tester};

// =============================================================================
// Enable Proposals & Proposals Enabled Tests
// =============================================================================

/// Test the full enable_proposals() flow and that proposals_enabled() returns true afterward.
#[xmtp_common::test(unwrap_try = true)]
async fn test_enable_proposals_and_proposals_enabled() {
    tester!(alix);
    tester!(bo);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;

    let bo_groups = bo.sync_welcomes().await?;
    let bo_group = bo_groups.first()?;
    bo_group.sync().await?;

    // Precondition: proposals not enabled
    let enabled_before = alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<bool, crate::groups::GroupError>(alix_group.proposals_enabled(&mls_group))
        })
        .await?;
    assert!(!enabled_before, "Proposals should not be enabled initially");

    // Enable proposals
    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;

    // Verify proposals_enabled returns true (tests proto decode + version > 0 path)
    let enabled_after = alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<bool, crate::groups::GroupError>(alix_group.proposals_enabled(&mls_group))
        })
        .await?;
    assert!(
        enabled_after,
        "Proposals should be enabled after enable_proposals()"
    );

    // Bo syncs and also sees proposals enabled
    bo_group.sync().await?;
    let bo_enabled = bo_group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<bool, crate::groups::GroupError>(bo_group.proposals_enabled(&mls_group))
        })
        .await?;
    assert!(bo_enabled, "Bo should also see proposals as enabled");
}

/// Test that enable_proposals() fails when a member doesn't support proposals.
#[xmtp_common::test(unwrap_try = true)]
async fn test_enable_proposals_fails_without_support() {
    use crate::identity::ENABLE_APP_DATA_DICTIONARY_BROADCAST;

    tester!(alix);

    // Create bo without proposal support by scoping the task local
    ENABLE_APP_DATA_DICTIONARY_BROADCAST
        .scope(false, async {
            tester!(bo);

            let alix_group = alix
                .create_group_with_members(&[bo.inbox_id()], None, None)
                .await
                .unwrap();

            bo.sync_welcomes().await.unwrap();

            // Bo doesn't support proposals, so all_members_support_proposals should be false
            let all_support = alix_group
                .load_mls_group_with_lock_async(async |mls_group| {
                    alix_group.all_members_support_proposals(&mls_group).await
                })
                .await
                .unwrap();
            assert!(!all_support, "Not all members should support proposals");

            // enable_proposals should fail
            let result = alix_group
                .enable_proposals(EnableProposalsOptions::test_default())
                .await;
            assert!(
                result.is_err(),
                "enable_proposals should fail when not all members support it"
            );
        })
        .await;
}

/// Test that adding a member without AppDataDictionary support to a
/// migrated group is rejected by OpenMLS. Post-bootstrap the group
/// context contains the `AppDataDictionary` extension and `RequiredCapabilities`
/// lists it, so every new leaf MUST advertise it in its key-package
/// capabilities.
///
/// Note: OpenMLS validates Add proposals against the CURRENT group context extensions
/// (validation.rs:395-404), so you cannot simultaneously remove an extension and add
/// a member who doesn't support it in the same commit. To add such a member, proposals
/// must be disabled first via a separate GCE commit.
#[xmtp_common::test(unwrap_try = true)]
async fn test_adding_unsupported_member_rejected_when_proposals_enabled() {
    use crate::identity::ENABLE_APP_DATA_DICTIONARY_BROADCAST;

    tester!(alix);
    tester!(bo);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;

    bo.sync_welcomes().await?;

    // Enable proposals (alix + bo both support it)
    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;

    let enabled = alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<bool, crate::groups::GroupError>(alix_group.proposals_enabled(&mls_group))
        })
        .await?;
    assert!(enabled, "Proposals should be enabled");

    // Adding a member without AppDataDictionary support should fail
    // because the migrated group context contains the AppDataDictionary
    // extension and OpenMLS requires new members to support all group
    // context extensions.
    ENABLE_APP_DATA_DICTIONARY_BROADCAST
        .scope(false, async {
            tester!(caro);

            let result = alix_group.add_members(&[caro.inbox_id()]).await;
            assert!(
                result.is_err(),
                "Adding unsupported member to proposal-enabled group should fail"
            );
        })
        .await;

    // Proposals should still be enabled (add was rejected)
    let still_enabled = alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<bool, crate::groups::GroupError>(alix_group.proposals_enabled(&mls_group))
        })
        .await?;
    assert!(
        still_enabled,
        "Proposals should still be enabled after failed add"
    );
}

/// Footgun guard: `enable_proposals` refuses to write a floor above the
/// caller's own pkg_version. Without this guard a developer who picked
/// the wrong constant could brick the group from the inside on the
/// very next sync.
#[xmtp_common::test(unwrap_try = true)]
async fn test_enable_proposals_rejects_min_version_above_own() {
    use crate::groups::GroupError;

    tester!(alix);

    let alix_group = alix.create_group(None, None)?;
    let result = alix_group
        .enable_proposals(EnableProposalsOptions {
            force: true,
            min_version: Some("99.0.0".to_string()),
        })
        .await;
    let err = result.expect_err("min_version above own pkg_version must be rejected");
    assert!(
        matches!(
            err,
            GroupError::MinVersionExceedsOwnVersion { ref requested, .. }
            if requested == "99.0.0"
        ),
        "expected MinVersionExceedsOwnVersion, got {err:?}",
    );
}

/// Send-side mirror of the receive-side downgrade check. Calling
/// `update_group_min_version` with a value below the existing floor
/// fails fast with a structured error rather than queueing a doomed
/// AppDataUpdate that every receiver would reject anyway.
#[xmtp_common::test(unwrap_try = true)]
async fn test_update_group_min_version_rejects_downgrade() {
    use crate::groups::GroupError;

    tester!(alix);
    tester!(bo);

    let alix_group = alix.create_group(None, None)?;
    alix_group
        .add_members(&[bo.context.identity.inbox_id()])
        .await?;
    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;

    // Raise the floor to alix's own pkg_version (legal — equal to own,
    // above the test_default 0.0.0 prior).
    let pkg = alix.version_info().pkg_version().to_string();
    alix_group.update_group_min_version(&pkg).await?;

    // Attempt to lower the floor to 0.0.0. Rejected before any intent
    // is queued.
    let err = alix_group
        .update_group_min_version("0.0.0")
        .await
        .expect_err("downgrade must be rejected by the send-side guard");
    assert!(
        matches!(
            err,
            GroupError::MinVersionDowngrade { ref requested, ref current }
            if requested == "0.0.0" && current == &pkg
        ),
        "expected MinVersionDowngrade, got {err:?}",
    );
}

/// `enable_proposals` is idempotent on an already-migrated group even
/// when the second call passes a forward-looking `min_version` that
/// would normally trip the above-own clamp. The clamp runs inside the
/// lock AFTER the `proposals_enabled` early-return so retry-on-failure
/// patterns can pin a constant without bricking idempotent callers.
#[xmtp_common::test(unwrap_try = true)]
async fn test_enable_proposals_idempotent_with_forward_min_version() {
    tester!(alix);
    tester!(bo);

    let alix_group = alix.create_group(None, None)?;
    alix_group
        .add_members(&[bo.context.identity.inbox_id()])
        .await?;
    // First call succeeds — group migrates with the test floor.
    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;
    // Second call with a min_version far above own pkg_version must
    // be a no-op (idempotent), NOT a MinVersionExceedsOwnVersion error.
    alix_group
        .enable_proposals(EnableProposalsOptions {
            force: true,
            min_version: Some("99.0.0".to_string()),
        })
        .await?;
}

/// `update_group_min_version` surfaces an unparseable input as a clean
/// `ProposalsNotSupported` error rather than leaking the internal
/// `CommitValidationError::InvalidVersionFormat` through the send-side
/// API. Pinned so a future refactor that drops the `map_err` wrapper
/// surfaces in CI.
#[xmtp_common::test(unwrap_try = true)]
async fn test_update_group_min_version_rejects_malformed_input() {
    use crate::groups::GroupError;

    tester!(alix);
    tester!(bo);

    let alix_group = alix.create_group(None, None)?;
    alix_group
        .add_members(&[bo.context.identity.inbox_id()])
        .await?;
    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;

    let err = alix_group
        .update_group_min_version("not-a-version")
        .await
        .expect_err("malformed semver must be rejected");
    assert!(
        matches!(
            err,
            GroupError::InvalidMinVersion { ref value, .. } if value == "not-a-version"
        ),
        "expected InvalidMinVersion, got {err:?}",
    );
}

/// Send-side mirror of the `enable_proposals` clamp on the steady-state
/// bump path: `update_group_min_version` also refuses values above own
/// pkg_version. Same footgun, same guard.
#[xmtp_common::test(unwrap_try = true)]
async fn test_update_group_min_version_rejects_above_own() {
    use crate::groups::GroupError;

    tester!(alix);
    tester!(bo);

    let alix_group = alix.create_group(None, None)?;
    alix_group
        .add_members(&[bo.context.identity.inbox_id()])
        .await?;
    alix_group
        .enable_proposals(EnableProposalsOptions::test_default())
        .await?;

    let err = alix_group
        .update_group_min_version("99.0.0")
        .await
        .expect_err("min_version above own pkg_version must be rejected");
    assert!(
        matches!(
            err,
            GroupError::MinVersionExceedsOwnVersion { ref requested, .. }
            if requested == "99.0.0"
        ),
        "expected MinVersionExceedsOwnVersion, got {err:?}",
    );
}

// =============================================================================
// Build Extensions Tests
// =============================================================================

/// Test that build_extensions_for_membership_update produces correct extensions
/// and doesn't mutate the original group.
#[xmtp_common::test(unwrap_try = true)]
async fn test_build_extensions_for_membership_update() {
    use crate::groups::{
        build_extensions_for_membership_update, validated_commit::extract_group_membership,
    };

    tester!(alix);
    tester!(bo);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;

    alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            // Get the current membership
            let current_membership = extract_group_membership(mls_group.extensions())?;
            let original_inbox_ids = current_membership.inbox_ids();

            // Build a new membership with an additional inbox
            let mut new_membership = current_membership.clone();
            new_membership.add("new_inbox_id".to_string(), 1);

            // Build updated extensions
            let updated_extensions =
                build_extensions_for_membership_update(&mls_group, &new_membership)?;

            // Verify the updated extensions contain the new membership
            let extracted = extract_group_membership(&updated_extensions)?;
            assert!(
                extracted.get("new_inbox_id").is_some(),
                "Updated extensions should contain the new inbox"
            );
            // Original members should still be present
            for inbox_id in &original_inbox_ids {
                assert!(
                    extracted.get(inbox_id).is_some(),
                    "Original member {} should still be present",
                    inbox_id
                );
            }

            // Verify original group extensions are unchanged (clone, not mutate)
            let unchanged = extract_group_membership(mls_group.extensions())?;
            assert!(
                unchanged.get("new_inbox_id").is_none(),
                "Original group extensions should not be mutated"
            );

            Ok::<(), crate::groups::GroupError>(())
        })
        .await?;
}
