use xmtp_id::InboxId;

use super::{validated_commit::extract_group_membership, GroupError, MlsGroup};

use crate::{
    storage::{
        association_state::StoredAssociationState,
        consent_record::{ConsentState, ConsentType},
    },
    xmtp_openmls_provider::XmtpOpenMlsProvider,
};

#[derive(Debug, Clone)]
pub struct GroupMember {
    pub inbox_id: InboxId,
    pub account_addresses: Vec<String>,
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

                let consent =
                    conn.get_consent_record(inbox_id_str.clone(), ConsentType::InboxId)?;

                Ok(GroupMember {
                    inbox_id: inbox_id_str.clone(),
                    account_addresses: association_state.account_addresses(),
                    installation_ids: association_state.installation_ids(),
                    permission_level,
                    consent_state: consent.map_or(ConsentState::Unknown, |c| c.state),
                })
            })
            .collect::<Result<Vec<GroupMember>, GroupError>>()?;

        Ok(members)
    }
}

#[cfg(test)]
mod tests {
    // use xmtp_cryptography::utils::generate_local_wallet;

    // use crate::builder::ClientBuilder;

    #[tokio::test]
    #[ignore]
    async fn test_member_list() {
        // let amal = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        // let bola_wallet = generate_local_wallet();
        // // Add two separate installations for Bola
        // let bola_a = ClientBuilder::new_test_client(&bola_wallet).await;
        // let bola_b = ClientBuilder::new_test_client(&bola_wallet).await;

        // let group = amal.create_group(None).unwrap();
        // Add both of Bola's installations to the group
        // group
        //     .add_members_by_installation_id(
        //         vec![
        //             bola_a.installation_public_key(),
        //             bola_b.installation_public_key(),
        //         ],
        //         &amal,
        //     )
        //     .await
        //     .unwrap();

        // let members = group.members().unwrap();
        // // The three installations should count as two members
        // assert_eq!(members.len(), 2);

        // for member in members {
        //     if member.account_address.eq(&amal.account_address()) {
        //         assert_eq!(member.installation_ids.len(), 1);
        //     }
        //     if member.account_address.eq(&bola_a.account_address()) {
        //         assert_eq!(member.installation_ids.len(), 2);
        //     }
        // }
    }
}
