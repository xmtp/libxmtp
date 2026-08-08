//! An inbox in the group membership dictionary can have no identity updates on
//! this network. The group must stay usable, and the member must be removable.
//! See xmtp/libxmtp#3952.
//!
//! A new installation still cannot join such a group. `check_initial_membership`
//! resolves every inbox in the dictionary before it accepts a welcome. Remove the
//! entry first. `test_unresolvable_member_can_be_removed` proves that order.

use crate::context::XmtpSharedContext;
use crate::groups::group_membership::GroupMembership;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::groups::validated_commit::extract_group_membership;
use crate::groups::{GroupError, build_group_membership_extension};
use crate::identity_updates::load_identity_updates;
use crate::tester;
use crate::utils::TestMlsGroup;
use crate::utils::test::FullXmtpClient;
use xmtp_common::RetryableError;
use xmtp_db::prelude::*;

/// Add an inbox to the group membership dictionary with no leaf in the tree.
/// The commit stays local, so no other member has to accept it. This copies a
/// group that a different client already jammed.
fn add_dict_entry(client: &FullXmtpClient, group: &TestMlsGroup, inbox_id: &str, sequence_id: u64) {
    let provider = client.context.mls_provider();

    group
        .load_mls_group_with_lock(client.context.mls_storage(), |mut mls_group| {
            let mut extensions = mls_group.extensions().clone();
            let mut membership = extract_group_membership(&extensions)?;
            membership.add(inbox_id.to_string(), sequence_id);
            extensions.add_or_replace(build_group_membership_extension(&membership))?;

            mls_group
                .update_group_context_extensions(
                    &provider,
                    extensions,
                    &client.identity().installation_keys,
                )
                .unwrap();
            mls_group.merge_pending_commit(&provider).unwrap();
            Ok(())
        })
        .unwrap();
}

/// Add an inbox that has no identity updates on this network.
fn jam_group(client: &FullXmtpClient, group: &TestMlsGroup) -> String {
    let unresolvable = hex::encode(xmtp_common::rand_array::<32>());
    add_dict_entry(client, group, &unresolvable, 1);
    unresolvable
}

fn membership(client: &FullXmtpClient, group: &TestMlsGroup) -> GroupMembership {
    group
        .load_mls_group_with_lock(client.context.mls_storage(), |mls_group| {
            Ok(extract_group_membership(mls_group.extensions())?)
        })
        .unwrap()
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_unresolvable_member_does_not_block_messages() {
    tester!(alix);
    let group = alix.create_group(None, None)?;
    let unresolvable = jam_group(&alix, &group);

    // This refresh reads every member of the dictionary. The jam stops it.
    group.update_installations().await?;
    group
        .send_message(b"hello", SendMessageOpts::default())
        .await?;

    // The skip keeps the entry. A drop is itself a membership change.
    assert!(membership(&alix, &group).get(&unresolvable).is_some());
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_unresolvable_member_does_not_block_other_members() {
    tester!(alix);
    tester!(bo);
    tester!(caro);
    let group = alix.create_group(None, None)?;
    let unresolvable = jam_group(&alix, &group);

    group.add_members(&[bo.inbox_id()]).await?;
    group.add_members(&[caro.inbox_id()]).await?;
    group.remove_members(&[bo.inbox_id()]).await?;

    let members = membership(&alix, &group);
    assert!(members.get(bo.inbox_id()).is_none());
    assert!(members.get(caro.inbox_id()).is_some());
    assert!(members.get(&unresolvable).is_some());
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_unresolvable_member_can_be_removed() {
    tester!(alix);
    tester!(bo);
    let group = alix.create_group(None, None)?;
    // Bring every sequence id up to date first. A stale id hides the defect,
    // because the refresh of that member also carries the removal.
    group.update_installations().await?;
    let unresolvable = jam_group(&alix, &group);

    group.remove_members(&[unresolvable.as_str()]).await?;
    assert!(membership(&alix, &group).get(&unresolvable).is_none());

    // The group works again. A new member joins and reads it.
    group.add_members(&[bo.inbox_id()]).await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    group.test_can_talk_with(&bo_group).await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_dict_entry_without_leaf_can_be_removed() {
    tester!(alix);
    tester!(mallory);
    let group = alix.create_group(None, None)?;
    group.update_installations().await?;

    // Alix resolves this inbox, but it has no leaf in the tree. The removal
    // commit drops no leaf, so the expectation must drop it too.
    let db = alix.context.db();
    load_identity_updates(alix.context.api(), &db, &[mallory.inbox_id()]).await?;
    let sequence_id = db.get_latest_sequence_id(&[mallory.inbox_id()])?[mallory.inbox_id()];
    add_dict_entry(&alix, &group, mallory.inbox_id(), sequence_id as u64);

    group.remove_members(&[mallory.inbox_id()]).await?;
    assert!(membership(&alix, &group).get(mallory.inbox_id()).is_none());
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_missing_sequence_id_is_only_for_unregistered_adds() {
    tester!(alix);
    let group = alix.create_group(None, None)?;

    // An add of an unregistered inbox still fails. The registration may arrive
    // later, so the error retries.
    let unregistered = hex::encode(xmtp_common::rand_array::<32>());
    let err = group
        .add_members(&[unregistered.as_str()])
        .await
        .unwrap_err();
    assert!(matches!(err, GroupError::MissingSequenceId));
    assert!(err.is_retryable());

    // A member that the group already committed raises no error at all.
    jam_group(&alix, &group);
    group.update_installations().await?;
}
