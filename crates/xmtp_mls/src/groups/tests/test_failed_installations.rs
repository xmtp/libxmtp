//! A key package can become unfetchable between intent-build and publish.
//! `GroupMembership::failed_installations` must still record it.

use crate::groups::GroupError;
use crate::groups::intents::QueueIntent;
use crate::groups::validated_commit::extract_group_membership;
use crate::tester;
use crate::utils::TestMlsGroup;
use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;
use xmtp_db::group::GroupQueryArgs;

/// `doomed_installation` must belong to an inbox that is already in the
/// group and that just gained an installation. Publish then raises that
/// inbox's sequence id. The membership therefore claims the installation.
async fn publish_update_with_publish_time_failure(
    alix_group: &TestMlsGroup,
    inbox_ids_to_add: &[&str],
    doomed_installation: &[u8],
) -> Result<(), GroupError> {
    // Build the intent while every key package still fetches and verifies.
    let intent_data = alix_group
        .get_membership_update_intent(inbox_ids_to_add, &[])
        .await?;
    assert!(
        !intent_data.is_empty(),
        "the new installation should produce a membership update"
    );
    assert!(
        intent_data.failed_installations.is_empty(),
        "every key package should still be fetchable when the intent is built"
    );

    // The key package now fails: rotation, expiry, or a bad verify.
    set_test_mode_upload_malformed_keypackage(true, Some(vec![doomed_installation.to_vec()]));

    let intent = QueueIntent::update_group_membership()
        .data(intent_data)
        .queue(alix_group)?;
    alix_group.sync_until_intent_resolved(intent.id).await?;

    Ok(())
}

#[xmtp_common::test(unwrap_try = true)]
async fn publish_time_key_package_failure_lands_in_membership() {
    tester!(alix);
    tester!(bo);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;

    // bo adds a second installation. This raises bo's identity sequence id.
    tester!(bo2, from: bo);
    let bo2_installation = bo2.context.installation_id().to_vec();

    publish_update_with_publish_time_failure(&alix_group, &[], &bo2_installation).await?;

    let membership = alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<_, GroupError>(extract_group_membership(mls_group.extensions())?)
        })
        .await?;

    assert!(
        membership.failed_installations.contains(&bo2_installation),
        "the publish-time key package failure must be recorded in the membership extension"
    );

    // The entry means something only if bo2 has no leaf.
    let leaf_installations = alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<_, GroupError>(
                mls_group
                    .members()
                    .map(|member| member.signature_key)
                    .collect::<Vec<_>>(),
            )
        })
        .await?;
    assert!(
        !leaf_installations.contains(&bo2_installation),
        "bo2 should have no leaf in the ratchet tree"
    );

    set_test_mode_upload_malformed_keypackage(false, None);
}

#[xmtp_common::test(unwrap_try = true)]
async fn joiner_accepts_welcome_with_publish_time_failed_installation() {
    tester!(alix);
    tester!(bo);
    tester!(caro);

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None)
        .await?;

    tester!(bo2, from: bo);
    let bo2_installation = bo2.context.installation_id().to_vec();

    // The same commit adds caro and fails on bo2. caro's welcome claims bo
    // at a sequence id that holds bo2. But bo2 has no leaf.
    publish_update_with_publish_time_failure(&alix_group, &[caro.inbox_id()], &bo2_installation)
        .await?;

    let caro_groups = caro.sync_welcomes().await?;
    assert_eq!(
        caro_groups.len(),
        1,
        "caro's welcome must not be rejected as InvalidGroupMembership"
    );
    assert_eq!(caro.find_groups(GroupQueryArgs::default())?.len(), 1);

    set_test_mode_upload_malformed_keypackage(false, None);
}
