use prost::Message;
use xmtp_db::XmtpOpenMlsProvider;
use xmtp_id::InboxIdRef;

use super::{IntentError, PostCommitAction};
use crate::groups::build_group_membership_extension;
use crate::groups::group_membership::MembershipDiffWithKeyPackages;
use crate::groups::mls_ext::MlsGroupExt;
use crate::groups::mls_ext::PublishIntentData;
use crate::groups::scoped_client::ScopedGroupClient;
use crate::groups::GroupError;
use crate::groups::{
    group_membership::GroupMembership, mls_ext::GroupIntent,
    mls_sync::calculate_membership_changes_with_keypackages,
};
use openmls::prelude::MlsGroup as OpenMlsGroup;
use std::collections::HashMap;
use tls_codec::Serialize;
use xmtp_proto::xmtp::mls::database::{
    update_group_membership_data::{
        Version as UpdateGroupMembershipVersion, V1 as UpdateGroupMembershipV1,
    },
    UpdateGroupMembershipData,
};

#[derive(Debug, Clone)]
pub struct MembershipIntentData {
    pub added_members: HashMap<String, u64>,
    pub removed_members: Vec<String>,
    pub failed_installations: Vec<Vec<u8>>,
}

impl MembershipIntentData {
    pub fn new(
        added_members: HashMap<String, u64>,
        removed_members: Vec<String>,
        failed_installations: Vec<Vec<u8>>,
    ) -> Self {
        Self {
            added_members,
            removed_members,
            failed_installations,
        }
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, IntentError> {
        if let UpdateGroupMembershipData {
            version: Some(UpdateGroupMembershipVersion::V1(v1)),
        } = UpdateGroupMembershipData::decode(bytes)?
        {
            Ok(Self::new(
                v1.membership_updates,
                v1.removed_members,
                v1.failed_installations,
            ))
        } else {
            Err(IntentError::MissingPayload)
        }
    }

    fn apply_identity_changes(&self, group_membership: &GroupMembership) -> GroupMembership {
        tracing::info!("old group membership: {:?}", group_membership.members);
        let mut new_membership = group_membership.clone();
        for (inbox_id, sequence_id) in self.added_members.iter() {
            new_membership.add(inbox_id.clone(), *sequence_id);
        }

        for inbox_id in self.removed_members.iter() {
            new_membership.remove(inbox_id)
        }
        tracing::info!("updated group membership: {:?}", new_membership.members);
        new_membership
    }

    fn apply_failed_installations(&self, group_membership: &GroupMembership) -> GroupMembership {
        let mut applied = group_membership.clone();
        applied
            .failed_installations
            .extend(self.failed_installations.clone());
        applied
    }

    pub fn is_empty(&self) -> bool {
        self.added_members.is_empty()
            && self.removed_members.is_empty()
            && self.failed_installations.is_empty()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct UpdateGroupMembershipIntent {
    pub data: MembershipIntentData,
    /// Diff of the members
    changes_with_kps: MembershipDiffWithKeyPackages,
    /// New Group Membership
    new_membership: GroupMembership,
    /// Old group membership
    old_membership: GroupMembership,
}

impl UpdateGroupMembershipIntent {
    /// Create a new membership update intent, and include any failed installations
    /// _from the network_
    pub async fn new(
        to_add: &[InboxIdRef<'_>],
        to_remove: &[InboxIdRef<'_>],
        changed_inbox_ids: HashMap<String, u64>,
        group: &OpenMlsGroup,
        client: impl ScopedGroupClient,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<Self, GroupError> {
        let to_remove = to_remove.into_iter().map(ToString::to_string).collect();
        let mut data = MembershipIntentData::new(changed_inbox_ids, to_remove, vec![]);
        let old_membership = group.membership()?;
        let new_membership = data.apply_identity_changes(&old_membership);
        let changes_with_kps = calculate_membership_changes_with_keypackages(
            client,
            provider,
            &new_membership,
            &old_membership,
        )
        .await?;
        data.failed_installations = changes_with_kps.failed_installations.clone();

        // If we fail to fetch or verify all the added members' KeyPackage, return an error.
        // skip if the inbox ids is 0 from the beginning
        if !to_add.is_empty()
            && !changes_with_kps.failed_installations.is_empty()
            && changes_with_kps.new_installations.is_empty()
        {
            return Err(GroupError::Intent(IntentError::FailedToVerifyInstallations));
        }

        Ok(Self {
            data,
            changes_with_kps,
            old_membership,
            new_membership,
        })
    }

    /// Created a new Membership Update Intent from bytes.
    /// This function applies all failed installations from deserialized bytes,
    /// and current group membership of the group
    pub async fn from_stored_bytes(
        data: &[u8],
        provider: &XmtpOpenMlsProvider,
        client: impl ScopedGroupClient,
        group: &OpenMlsGroup,
    ) -> Result<Self, GroupError> {
        let data = MembershipIntentData::from_bytes(data)?;
        let old_group_membership = group.membership()?;
        let new_group_membership = data.apply_identity_changes(&old_group_membership);
        let new_group_membership = data.apply_failed_installations(&new_group_membership);
        let changes_with_kps = calculate_membership_changes_with_keypackages(
            client,
            provider,
            &new_group_membership,
            &old_group_membership,
        )
        .await?;

        Ok(Self {
            data,
            changes_with_kps,
            new_membership: new_group_membership,
            old_membership: old_group_membership,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

// Takes UpdateGroupMembershipIntent and applies it to the openmls group
// returning the commit and post_commit_action
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl GroupIntent for UpdateGroupMembershipIntent {
    async fn publish_data(
        self: Box<Self>,
        provider: &xmtp_db::XmtpOpenMlsProvider,
        context: &crate::client::XmtpMlsLocalContext,
        group: &mut OpenMlsGroup,
        should_push: bool,
    ) -> Result<Option<crate::groups::mls_ext::PublishIntentData>, crate::groups::GroupError> {
        let extensions = group.extensions();
        let membership_diff = self.old_membership.diff(&self.new_membership);
        let leaf_nodes_to_remove =
            group.removed_leaf_nodes(&self.changes_with_kps.removed_installations);

        if leaf_nodes_to_remove.is_empty()
            && self.changes_with_kps.new_key_packages.is_empty()
            && membership_diff.updated_inboxes.is_empty()
        {
            return Ok(None);
        }

        // Update the extensions to have the new GroupMembership
        let mut new_extensions = extensions.clone();

        new_extensions.add_or_replace(build_group_membership_extension(&self.new_membership));

        // Create the commit
        let (commit, maybe_welcome_message, _) = group.update_group_membership(
            provider,
            &context.identity.installation_keys,
            &self.changes_with_kps.new_key_packages,
            &leaf_nodes_to_remove,
            new_extensions,
        )?;

        let post_commit_action = maybe_welcome_message
            .map(|w| PostCommitAction::from_welcome(w, self.changes_with_kps.new_installations))
            .transpose()?;

        let staged_commit = group
            .get_and_clear_pending_commit(provider)?
            .ok_or_else(|| GroupError::MissingPendingCommit)?;

        PublishIntentData::builder()
            .payload(commit.tls_serialize_detached()?)
            .post_commit_action(post_commit_action.map(|action| action.to_bytes()))
            .staged_commit(staged_commit)
            .should_push(should_push)
            .build()
            .map_err(GroupError::from)
            .map(Option::Some)
    }
}

impl From<UpdateGroupMembershipIntent> for Vec<u8> {
    fn from(intent: UpdateGroupMembershipIntent) -> Self {
        let mut buf = Vec::new();

        UpdateGroupMembershipData {
            version: Some(UpdateGroupMembershipVersion::V1(UpdateGroupMembershipV1 {
                membership_updates: intent.data.added_members,
                removed_members: intent.data.removed_members,
                failed_installations: intent.data.failed_installations,
            })),
        }
        .encode(&mut buf)
        .expect("encode error");

        buf
    }
}
