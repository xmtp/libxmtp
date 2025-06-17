use super::*;
use crate::context::XmtpContextProvider;
use crate::context::XmtpMlsLocalContext;
use crate::groups::{
    build_group_membership_extension,
    intents::{PostCommitAction, UpdateGroupMembershipIntentData},
    validated_commit::extract_group_membership,
    GroupError,
};
use openmls::{
    extensions::Extensions,
    prelude::{tls_codec::Serialize, LeafNodeIndex, MlsGroup as OpenMlsGroup},
};
use openmls_traits::signatures::Signer;
use std::sync::Arc;
use xmtp_api::XmtpApi;
use xmtp_db::MlsProviderExt;

// Takes UpdateGroupMembershipIntentData and applies it to the openmls group
// returning the commit and post_commit_action
#[tracing::instrument(level = "trace", skip_all)]
pub async fn apply_update_group_membership_intent<Context>(
    context: &Arc<XmtpMlsLocalContext<Context>>,
    openmls_group: &mut OpenMlsGroup,
    intent_data: UpdateGroupMembershipIntentData,
    signer: impl Signer,
) -> Result<Option<PublishIntentData>, GroupError>
where
    Context: XmtpShared,
{
    let provider = context.mls_provider();
    let extensions: Extensions = openmls_group.extensions().clone();
    let old_group_membership = extract_group_membership(&extensions)?;
    let new_group_membership = intent_data.apply_to_group_membership(&old_group_membership);
    let membership_diff = old_group_membership.diff(&new_group_membership);

    let changes_with_kps = calculate_membership_changes_with_keypackages(
        context.clone(),
        &new_group_membership,
        &old_group_membership,
    )
    .await?;
    let leaf_nodes_to_remove: Vec<LeafNodeIndex> =
        get_removed_leaf_nodes(openmls_group, &changes_with_kps.removed_installations);

    if leaf_nodes_to_remove.contains(&openmls_group.own_leaf_index()) {
        tracing::info!("Cannot remove own leaf node");
        return Ok(None);
    }

    if leaf_nodes_to_remove.is_empty()
        && changes_with_kps.new_key_packages.is_empty()
        && membership_diff.updated_inboxes.is_empty()
    {
        return Ok(None);
    }

    // Update the extensions to have the new GroupMembership
    let mut new_extensions = extensions.clone();

    new_extensions.add_or_replace(build_group_membership_extension(&new_group_membership));

    let (commit, post_commit_action, staged_commit) = provider.transaction(|provider| {
        // Create the commit
        let (commit, maybe_welcome_message, _) = openmls_group.update_group_membership(
            &provider,
            &signer,
            &changes_with_kps.new_key_packages,
            &leaf_nodes_to_remove,
            new_extensions,
        )?;

        let post_commit_action = match maybe_welcome_message {
            Some(welcome_message) => Some(PostCommitAction::from_welcome(
                welcome_message,
                changes_with_kps.new_installations,
            )?),
            None => None,
        };

        let staged_commit = get_and_clear_pending_commit(openmls_group, provider)?
            .ok_or_else(|| GroupError::MissingPendingCommit)?;

        Ok::<_, GroupError>((commit, post_commit_action, staged_commit))
    })?;

    Ok(Some(PublishIntentData {
        payload_to_publish: commit.tls_serialize_detached()?,
        post_commit_action: post_commit_action.map(|action| action.to_bytes()),
        staged_commit: Some(staged_commit),
        should_send_push_notification: false,
    }))
}
