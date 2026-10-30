//! An installation the sender cannot add must land in
//! `GroupMembership::failed_installations`: whether its key package is
//! absent from the backend, or becomes unfetchable between intent-build and
//! publish.

use std::collections::HashSet;

use crate::groups::GroupError;
use crate::groups::intents::QueueIntent;
use crate::tester;
use crate::utils::TestMlsGroup;
use crate::utils::test_mocks_helpers::{
    RestoreKeyPackages, set_test_mode_upload_malformed_keypackage,
};
use xmtp_cryptography::{CredentialSign, XmtpInstallationCredential};
use xmtp_db::group::GroupQueryArgs;
use xmtp_id::associations::builder::SignatureRequestBuilder;
use xmtp_id::associations::test_utils::{WalletTestExt, add_wallet_signature};
use xmtp_id::associations::unverified::UnverifiedSignature;
use xmtp_id::associations::{InstallationKeyContext, MemberIdentifier};
use xmtp_mls_validation::commit::extract_group_membership;

/// Requirement: when the backend serves no key package for one of an inbox's
/// installations, the sender adds the inbox with its other installations and
/// records the absent one as failed. Every installation in the inbox is then
/// a leaf or a failed installation, so a joiner accepts the Welcome. Dropping
/// the absent installation silently would make every joiner reject it.
// verifies: JOIN-093, JOIN-012, GMOD-009
#[xmtp_common::test(unwrap_try = true)]
async fn missing_package_is_recorded() {
    tester!(alix);
    tester!(bo);
    tester!(caro);

    // The same absent installation belongs to both inboxes; both retain a primary.
    let absent = XmtpInstallationCredential::new();
    let absent_id = absent.public_slice().to_vec();
    for member in [&bo, &caro] {
        let mut request = SignatureRequestBuilder::new(member.inbox_id())
            .add_association(
                MemberIdentifier::installation(absent_id.clone()),
                member.builder.owner.identifier().into(),
            )
            .build();
        let signature =
            absent.credential_sign::<InstallationKeyContext>(request.signature_text())?;
        request
            .add_signature(
                UnverifiedSignature::new_installation_key(signature, absent.verifying_key()),
                &member.context.scw_verifier(),
            )
            .await?;
        add_wallet_signature(&mut request, &member.builder.owner).await;
        member
            .identity_updates()
            .apply_signature_request(request)
            .await?;
    }

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;

    let ((membership, failed), leaves) = alix_group
        .load_mls_group_with_lock_async(async |mls_group| {
            Ok::<_, GroupError>((
                xmtp_mls_validation::commit::extract_group_membership_with_failures(
                    mls_group.extensions(),
                )?,
                mls_group
                    .members()
                    .map(|member| member.signature_key)
                    .collect::<HashSet<_>>(),
            ))
        })
        .await?;
    assert_eq!(
        membership
            .failed_installations
            .iter()
            .collect::<HashSet<_>>(),
        HashSet::from([&absent_id]),
        "the installation with no key package must be recorded as failed"
    );
    assert!(
        !leaves.contains(&absent_id),
        "the absent installation must have no leaf"
    );
    for joiner in [&bo, &caro] {
        assert!(leaves.contains(joiner.installation_public_key().as_slice()));
        assert!(
            failed
                .of(joiner.inbox_id())
                .is_some_and(|keys| keys.contains(&absent_id)),
            "each inbox that owns the absent installation must list its failure"
        );
        let state = joiner
            .identity_updates()
            .get_latest_association_state(&joiner.context.db(), joiner.inbox_id())
            .await?;
        assert_eq!(state.installation_ids().len(), 2);
        assert!(state.installation_ids().iter().all(|key| {
            leaves.contains(key)
                || failed
                    .of(joiner.inbox_id())
                    .is_some_and(|keys| keys.contains(key))
        }));
    }
    for joiner in [&bo, &caro] {
        xmtp_common::wait_for_eq(
            || async {
                joiner.sync_welcomes().await.unwrap();
                joiner
                    .group(&alix_group.group_id)
                    .is_ok_and(|group| group.is_active().unwrap())
            },
            true,
        )
        .await?;
        assert_eq!(
            joiner.find_groups(GroupQueryArgs::default())?.len(),
            1,
            "a joiner must accept a Welcome that accounts for every inbox's absent installation"
        );
    }
}

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

// verifies: GMOD-047, PERM-030, PERM-015
#[rstest::rstest]
#[case::direct(false)]
#[case::pending_by_other_member(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn leafless_inbox_removal_uses_delete_proposer(
    #[case] pending: bool,
) -> Result<(), GroupError> {
    use xmtp_db::incoming_envelope::QueryIncomingEnvelope;
    tester!(alix);
    tester!(bo);
    tester!(caro);
    let group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await?;
    tester!(bo2, from: bo, disable_workers);
    let revoked_installation = bo.context.installation_id().to_vec();
    let mut revoke = bo
        .identity_updates()
        .revoke_installations(vec![revoked_installation.clone()])
        .await?;
    add_wallet_signature(&mut revoke, &bo.builder.owner).await;
    bo.identity_updates()
        .apply_signature_request(revoke)
        .await?;
    let bo_installation = bo2.context.installation_id().to_vec();
    let _restore = RestoreKeyPackages;
    set_test_mode_upload_malformed_keypackage(true, Some(vec![bo_installation.clone()]));
    group.add_missing_installations().await?;
    let (membership, leaves) = group
        .load_mls_group_with_lock_async(async |mls| {
            Ok::<_, GroupError>((
                extract_group_membership(mls.extensions())?,
                mls.members()
                    .map(|member| member.signature_key)
                    .collect::<HashSet<_>>(),
            ))
        })
        .await?;
    assert!(membership.get(bo.inbox_id()).is_some());
    assert!(membership.failed_installations.contains(&bo_installation));
    assert!(!leaves.contains(&bo_installation));
    assert!(!leaves.contains(&revoked_installation));
    assert_eq!(group.members().await?.len(), 3);
    xmtp_common::wait_for_eq(
        || async {
            caro.sync_welcomes().await.unwrap();
            caro.group(&group.group_id)
                .is_ok_and(|group| group.is_active().unwrap())
        },
        true,
    )
    .await
    .unwrap();
    let caro_group = caro.group(&group.group_id)?;
    caro_group.sync().await?;
    if pending {
        // Caro commits Alix's authorized proposal; Caro is not an admin.
        let intent = QueueIntent::propose_member_update()
            .data(Vec::<u8>::try_from(
                crate::groups::intents::ProposeMemberUpdateIntentData::new(
                    vec![],
                    vec![bo.inbox_id().to_string()],
                ),
            )?)
            .queue(&group)?;
        group.sync_until_intent_resolved(intent.id).await?;
        let cursor = group
            .context
            .db()
            .topic_progress(&xmtp_db::incoming_envelope::StreamTopic::group(
                group.group_id,
            ))?
            .processed;
        crate::subscriptions::barrier::wait_through(
            &caro_group.context,
            [(
                xmtp_proto::types::Topic::new_group_message(group.group_id),
                cursor,
            )]
            .into(),
            None,
        )
        .await?;
        let intent = QueueIntent::commit_pending_proposals().queue(&caro_group)?;
        caro_group.sync_until_intent_resolved(intent.id).await?;
    } else {
        group.remove_members(&[bo.inbox_id()]).await?;
    }
    let (author, receiver) = if pending {
        (&caro_group, &group)
    } else {
        (&group, &caro_group)
    };
    let cursor = author
        .context
        .db()
        .topic_progress(&xmtp_db::incoming_envelope::StreamTopic::group(
            group.group_id,
        ))?
        .processed;
    crate::subscriptions::barrier::wait_through(
        &receiver.context,
        [(
            xmtp_proto::types::Topic::new_group_message(group.group_id),
            cursor,
        )]
        .into(),
        None,
    )
    .await?;
    for member_group in [&group, &caro_group] {
        let membership = member_group
            .load_mls_group_with_lock_async(async |mls| {
                Ok::<_, GroupError>(extract_group_membership(mls.extensions())?)
            })
            .await?;
        assert!(membership.get(bo.inbox_id()).is_none());
        assert_eq!(member_group.members().await?.len(), 2);
    }
    Ok(())
}
