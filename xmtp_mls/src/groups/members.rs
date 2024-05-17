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
}

impl MlsGroup {
    // Load the member list for the group from the DB, merging together multiple installations into a single entry
    pub fn members(&self) -> Result<Vec<GroupMember>, GroupError> {
        let conn = self.context.store.conn()?;
        let provider = self.context.mls_provider(conn);
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
            .collect();

        let conn = provider.conn_ref();
        let association_state_map = StoredAssociationState::batch_read_from_cache(conn, &requests)?;
        // TODO: Figure out what to do with missing members from the local DB. Do we go to the network? Load from identity updates?
        // Right now I am just omitting them
        let members = association_state_map
            .into_iter()
            .map(|association_state| GroupMember {
                inbox_id: association_state.inbox_id().to_string(),
                account_addresses: association_state.account_addresses(),
                installation_ids: association_state.installation_ids(),
            })
            .collect::<Vec<GroupMember>>();

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
