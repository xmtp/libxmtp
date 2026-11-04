//! The leaves of `groups/tests/test_failed_installations.rs` that need the client.

use crate::groups::GroupError;
use crate::tester;
use std::collections::HashSet;
use xmtp_cryptography::{CredentialSign, XmtpInstallationCredential};
use xmtp_db::group::GroupQueryArgs;
use xmtp_id::associations::builder::SignatureRequestBuilder;
use xmtp_id::associations::test_utils::{WalletTestExt, add_wallet_signature};
use xmtp_id::associations::unverified::UnverifiedSignature;
use xmtp_id::associations::{InstallationKeyContext, MemberIdentifier};

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
