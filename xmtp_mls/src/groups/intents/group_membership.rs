use openmls::prelude::Extensions;
use prost::Message;

use super::{IntentError, PostCommitAction};
use crate::groups::build_group_membership_extension;
use crate::groups::mls_ext::MlsExtensionsExt;
use crate::groups::mls_ext::MlsGroupExt;
use crate::groups::mls_sync::PublishIntentData;
use crate::groups::GroupError;
use crate::groups::{
    group_membership::GroupMembership, mls_ext::GroupIntent,
    mls_sync::calculate_membership_changes_with_keypackages,
};
use openmls::prelude::MlsGroup as OpenMlsGroup;
use std::collections::{HashMap, HashSet};
use tls_codec::Serialize;
use xmtp_proto::xmtp::mls::database::{
    update_group_membership_data::{
        Version as UpdateGroupMembershipVersion, V1 as UpdateGroupMembershipV1,
    },
    UpdateGroupMembershipData,
};

#[derive(Debug, Default, Clone)]
pub struct UpdateGroupMembershipResult {
    pub added_members: HashMap<String, u64>,
    pub removed_members: Vec<String>,
    pub failed_installations: Vec<Vec<u8>>,
}

impl UpdateGroupMembershipResult {
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
}

impl From<UpdateGroupMembershipIntentData> for UpdateGroupMembershipResult {
    fn from(value: UpdateGroupMembershipIntentData) -> Self {
        UpdateGroupMembershipResult::new(
            value.membership_updates,
            value.removed_members,
            value.failed_installations,
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) struct UpdateGroupMembershipIntentData {
    pub membership_updates: HashMap<String, u64>,
    pub removed_members: Vec<String>,
    pub failed_installations: Vec<Vec<u8>>,
}

impl UpdateGroupMembershipIntentData {
    pub fn new(
        membership_updates: HashMap<String, u64>,
        removed_members: Vec<String>,
        failed_installations: Vec<Vec<u8>>,
    ) -> Self {
        Self {
            membership_updates,
            removed_members,
            failed_installations,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.membership_updates.is_empty()
            && self.removed_members.is_empty()
            && self.failed_installations.is_empty()
    }

    pub fn apply_to_group_membership(&self, group_membership: &GroupMembership) -> GroupMembership {
        tracing::info!("old group membership: {:?}", group_membership.members);
        let mut new_membership = group_membership.clone();
        for (inbox_id, sequence_id) in self.membership_updates.iter() {
            new_membership.add(inbox_id.clone(), *sequence_id);
        }

        for inbox_id in self.removed_members.iter() {
            new_membership.remove(inbox_id)
        }

        new_membership.failed_installations = new_membership
            .failed_installations
            .into_iter()
            .chain(self.failed_installations.iter().cloned())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        tracing::info!("updated group membership: {:?}", new_membership.members);
        new_membership
    }
}

impl GroupIntent for UpdateGroupMembershipIntentData {
    async fn publish_data(
        &self,
        provider: &xmtp_db::XmtpOpenMlsProvider,
        client: impl crate::groups::scoped_client::ScopedGroupClient,
        context: &crate::client::XmtpMlsLocalContext,
        group: &mut OpenMlsGroup,
    ) -> Result<Option<crate::groups::mls_sync::PublishIntentData>, crate::groups::GroupError> {
        let extensions: Extensions = group.extensions().clone();
        let old_group_membership = extensions.group_membership()?;
        let new_group_membership = self.apply_to_group_membership(&old_group_membership);
        let membership_diff = old_group_membership.diff(&new_group_membership);

        let changes_with_kps = calculate_membership_changes_with_keypackages(
            client,
            provider,
            &new_group_membership,
            &old_group_membership,
        )
        .await?;
        let leaf_nodes_to_remove =
            group.removed_leaf_nodes(&changes_with_kps.removed_installations);

        if leaf_nodes_to_remove.is_empty()
            && changes_with_kps.new_key_packages.is_empty()
            && membership_diff.updated_inboxes.is_empty()
        {
            return Ok(None);
        }

        // Update the extensions to have the new GroupMembership
        let mut new_extensions = extensions.clone();

        new_extensions.add_or_replace(build_group_membership_extension(&new_group_membership));

        // Create the commit
        let (commit, maybe_welcome_message, _) = group.update_group_membership(
            provider,
            &context.identity.installation_keys,
            &changes_with_kps.new_key_packages,
            &leaf_nodes_to_remove,
            new_extensions,
        )?;

        let post_commit_action = maybe_welcome_message
            .map(|w| PostCommitAction::from_welcome(w, changes_with_kps.new_installations))
            .transpose()?;

        let staged_commit = group
            .get_and_clear_pending_commit(provider)?
            .ok_or_else(|| GroupError::MissingPendingCommit)?;

        Ok(Some(PublishIntentData {
            payload_to_publish: commit.tls_serialize_detached()?,
            post_commit_action: post_commit_action.map(|action| action.to_bytes()),
            staged_commit: Some(staged_commit),
            should_send_push_notification: false,
        }))
    }
}

impl From<UpdateGroupMembershipIntentData> for Vec<u8> {
    fn from(intent: UpdateGroupMembershipIntentData) -> Self {
        let mut buf = Vec::new();

        UpdateGroupMembershipData {
            version: Some(UpdateGroupMembershipVersion::V1(UpdateGroupMembershipV1 {
                membership_updates: intent.membership_updates,
                removed_members: intent.removed_members,
                failed_installations: intent.failed_installations,
            })),
        }
        .encode(&mut buf)
        .expect("encode error");

        buf
    }
}

impl TryFrom<Vec<u8>> for UpdateGroupMembershipIntentData {
    type Error = IntentError;

    fn try_from(data: Vec<u8>) -> Result<Self, Self::Error> {
        if let UpdateGroupMembershipData {
            version: Some(UpdateGroupMembershipVersion::V1(v1)),
        } = UpdateGroupMembershipData::decode(data.as_slice())?
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
}

impl<'a> TryFrom<&'a [u8]> for UpdateGroupMembershipIntentData {
    type Error = IntentError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        if let UpdateGroupMembershipData {
            version: Some(UpdateGroupMembershipVersion::V1(v1)),
        } = UpdateGroupMembershipData::decode(data)?
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
}
