use super::{validated_commit::extract_group_membership, GroupError, MlsGroup, ScopedGroupClient};
use crate::storage::{
    association_state::StoredAssociationState,
    consent_record::{ConsentEntity, ConsentState},
    xmtp_openmls_provider::XmtpOpenMlsProvider,
};
use xmtp_id::{associations::PublicIdentifier, InboxId};

#[derive(Debug, Clone)]
pub struct GroupMember {
    pub inbox_id: InboxId,
    pub account_identifiers: Vec<PublicIdentifier>,
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

impl<ScopedClient> MlsGroup<ScopedClient>
where
    ScopedClient: ScopedGroupClient,
{
    // Load the member list for the group from the DB, merging together multiple installations into a single entry
    pub async fn members(&self) -> Result<Vec<GroupMember>, GroupError> {
        let provider = self.mls_provider()?;
        self.members_with_provider(&provider).await
    }

    pub async fn members_with_provider(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<Vec<GroupMember>, GroupError> {
        let group_membership = self.load_mls_group_with_lock(provider, |mls_group| {
            Ok(extract_group_membership(mls_group.extensions())?)
        })?;
        let requests = group_membership
            .members
            .into_iter()
            .map(|(inbox_id, sequence_id)| (inbox_id, sequence_id as i64))
            .filter(|(_, sequence_id)| *sequence_id != 0) // Skip the initial state
            .collect::<Vec<_>>();

        let conn = provider.conn_ref();
        let mut association_states =
            StoredAssociationState::batch_read_from_cache(conn, requests.clone())?;
        let mutable_metadata = self.mutable_metadata(provider)?;
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

            let mut new_states = self
                .client
                .batch_get_association_state(conn, &missing_requests)
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
        let members = association_states
            .into_iter()
            .map(|association_state| {
                let inbox_id = association_state.inbox_id().to_string();
                let is_admin = mutable_metadata.is_admin(&inbox_id);
                let is_super_admin = mutable_metadata.is_super_admin(&inbox_id);
                let permission_level = if is_super_admin {
                    PermissionLevel::SuperAdmin
                } else if is_admin {
                    PermissionLevel::Admin
                } else {
                    PermissionLevel::Member
                };

                let consent = conn.get_consent_record(&ConsentEntity::InboxId(inbox_id.clone()))?;

                Ok(GroupMember {
                    inbox_id: inbox_id.clone(),
                    account_identifiers: association_state.public_identifiers(),
                    installation_ids: association_state.installation_ids(),
                    permission_level,
                    consent_state: consent.map_or(ConsentState::Unknown, |c| c.state),
                })
            })
            .collect::<Result<Vec<GroupMember>, GroupError>>()?;

        Ok(members)
    }
}
