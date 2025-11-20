use crate::{context::XmtpSharedContext, identity_updates::IdentityUpdates};

use super::{GroupError, MlsGroup, validated_commit::extract_group_membership};
use xmtp_db::prelude::*;
use xmtp_db::{
    StorageError,
    consent_record::{ConsentState, ConsentType},
};
use xmtp_id::{
    InboxId,
    associations::{AssociationState, Identifier},
};

#[derive(Debug, Clone)]
pub struct GroupMember {
    pub inbox_id: InboxId,
    pub account_identifiers: Vec<Identifier>,
    pub installation_ids: Vec<Vec<u8>>,
    pub permission_level: PermissionLevel,
    pub consent_state: ConsentState,
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionLevel {
    Member,
    Admin,
    SuperAdmin,
}

impl<Context> MlsGroup<Context>
where
    Context: XmtpSharedContext,
{
    /// Load the member list for the group from the DB, merging together multiple installations into a single entry
    pub async fn members(&self) -> Result<Vec<GroupMember>, GroupError> {
        let db = self.context.db();
        let storage = self.context.mls_storage();
        let group_membership = self.load_mls_group_with_lock(storage, |mls_group| {
            Ok(extract_group_membership(mls_group.extensions())?)
        })?;
        let requests = group_membership
            .members
            .into_iter()
            .map(|(inbox_id, sequence_id)| (inbox_id, sequence_id as i64))
            .filter(|(_, sequence_id)| *sequence_id != 0) // Skip the initial state
            .collect::<Vec<_>>();

        let association_states = db.batch_read_from_cache(requests.clone())?;
        let mut association_states: Vec<AssociationState> = association_states
            .into_iter()
            .map(|a| a.try_into())
            .collect::<Result<_, _>>()
            .map_err(StorageError::from)?;
        if association_states.len() != requests.len() {
            // Attempt to rebuild the cache.
            let missing_requests: Vec<_> = requests
                .iter()
                .filter_map(|(id, sequence)| {
                    // Filter out association states we already have to avoid unnecessary requests.
                    if association_states
                        .iter()
                        .any(|state| state.inbox_id() == id)
                    {
                        return None;
                    }
                    Some((id.as_str(), Some(*sequence)))
                })
                .collect();
            let identity_updates = IdentityUpdates::new(&self.context);
            let mut new_states = identity_updates
                .batch_get_association_state(&db, &missing_requests)
                .await?;
            association_states.append(&mut new_states);

            if association_states.len() != requests.len() {
                // Cache miss - not expected to happen because:
                // 1. We don't allow updates to the group metadata unless we have already validated the association state
                // 2. When validating the association state, we must have written it to the cache
                tracing::error!(
                    "Failed to load all members for group - metadata: {:?}, computed members: {:?}",
                    requests,
                    association_states
                );
                return Err(GroupError::InvalidGroupMembership);
            }
        }
        let mutable_metadata = self.mutable_metadata()?;
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

                let consent = db.get_consent_record(inbox_id_str.clone(), ConsentType::InboxId)?;

                Ok(GroupMember {
                    inbox_id: inbox_id_str.clone(),
                    account_identifiers: association_state.identifiers(),
                    installation_ids: association_state.installation_ids(),
                    permission_level,
                    consent_state: consent.map_or(ConsentState::Unknown, |c| c.state),
                })
            })
            .collect::<Result<Vec<GroupMember>, GroupError>>()?;

        Ok(members)
    }
}
