use xmtp_id::InboxId;

use super::{validated_commit::extract_group_membership, GroupError, MlsGroup};

use crate::{
    storage::association_state::StoredAssociationState, xmtp_openmls_provider::XmtpOpenMlsProvider,
};

#[derive(Debug, Clone)]
pub struct GroupMember {
    pub inbox_id: InboxId,
    pub account_addresses: Vec<String>,
    pub installation_ids: Vec<Vec<u8>>,
    pub permission_level: PermissionLevel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionLevel {
    Member,
    Admin,
    SuperAdmin,
}

impl MlsGroup {
    // Load the member list for the group from the DB, merging together multiple installations into a single entry
    pub fn members(&self) -> Result<Vec<GroupMember>, GroupError> {
        let provider = self.mls_provider()?;
        self.members_with_provider(&provider)
    }

    pub fn members_with_provider(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<Vec<GroupMember>, GroupError> {
        let openmls_group = self.load_mls_group(provider)?;
        // TODO: Replace with try_into from extensions
        let group_membership = extract_group_membership(openmls_group.extensions())?;
        let requests = group_membership
            .members
            .into_iter()
            .map(|(inbox_id, sequence_id)| (inbox_id, sequence_id as i64))
            .filter(|(_, sequence_id)| *sequence_id != 0) // Skip the initial state
            .collect::<Vec<_>>();

        let conn = provider.conn_ref();
        let association_states =
            StoredAssociationState::batch_read_from_cache(conn, requests.clone())?;
        let mutable_metadata = self.mutable_metadata(provider)?;
        if association_states.len() != requests.len() {
            // Cache miss - not expected to happen because:
            // 1. We don't allow updates to the group metadata unless we have already validated the association state
            // 2. When validating the association state, we must have written it to the cache
            log::error!(
                "Failed to load all members for group - metadata: {:?}, computed members: {:?}",
                requests,
                association_states
            );
            return Err(GroupError::InvalidGroupMembership);
        }
        let members = association_states
            .into_iter()
            .map(|association_state| {
                let inbox_id_str = association_state.inbox_id().to_string();
                let is_admin = mutable_metadata.is_admin(&inbox_id_str);
                let is_super_admin = mutable_metadata.is_super_admin(&inbox_id_str);
                let permission_level = if is_super_admin {
                    PermissionLevel::SuperAdmin
                } else if is_admin {
                    PermissionLevel::Admin
                } else {
                    PermissionLevel::Member
                };

                Ok(GroupMember {
                    inbox_id: inbox_id_str,
                    account_addresses: association_state.account_addresses(),
                    installation_ids: association_state.installation_ids(),
                    permission_level,
                })
            })
            .collect::<Result<Vec<GroupMember>, GroupError>>()?;

        Ok(members)
    }
}
