use std::collections::HashMap;

use openmls::{
    group::{QueuedAddProposal, QueuedRemoveProposal},
    prelude::{MlsGroup as OpenMlsGroup, StagedCommit},
};
use xmtp_proto::{
    api_client::{XmtpApiClient, XmtpMlsClient},
    xmtp::mls::message_contents::{GroupMembershipChange, Member as MemberProto},
};

use crate::identity::Identity;

use super::{GroupError, MlsGroup};

// Take a QueuedAddProposal and extract the wallet address and installation_id
fn extract_identity_from_add(proposal: QueuedAddProposal) -> Option<(String, Vec<u8>)> {
    let leaf_node = proposal.add_proposal().key_package().leaf_node();
    let signature_key = leaf_node.signature_key().as_slice();
    match Identity::get_validated_account_address(leaf_node.credential().identity(), signature_key)
    {
        Ok(wallet_address) => Some((wallet_address, signature_key.to_vec())),
        Err(err) => {
            log::warn!("error extracting identity {}", err);
            None
        }
    }
}

// Take a QueuedRemoveProposal and extract the wallet address and installation_id
fn extract_identity_from_remove(
    proposal: QueuedRemoveProposal,
    group: &OpenMlsGroup,
) -> Option<(String, Vec<u8>)> {
    let leaf_index = proposal.remove_proposal().removed();
    let maybe_member = group.member_at(leaf_index);
    if maybe_member.is_none() {
        log::warn!("could not find removed member");
        return None;
    }
    let member = maybe_member.expect("already checked");
    let signature_key = member.signature_key.as_slice();
    match Identity::get_validated_account_address(member.credential.identity(), signature_key) {
        Ok(wallet_address) => Some((wallet_address, signature_key.to_vec())),
        Err(err) => {
            log::warn!("error extracting identity {}", err);
            None
        }
    }
}

// Reducer function for merging members into a map, with all installation_ids collected per member
fn merge_members(
    mut acc: HashMap<String, MemberProto>,
    (wallet_address, signature_key): (String, Vec<u8>),
) -> HashMap<String, MemberProto> {
    acc.entry(wallet_address.clone())
        .and_modify(|entry| entry.installation_ids.push(signature_key.clone()))
        .or_insert(MemberProto {
            wallet_address,
            installation_ids: vec![signature_key],
        });
    acc
}

// Get a tuple of (new_members, new_installations), each formatted as a Member object with all installation_ids grouped
fn get_new_members(
    staged_commit: &StagedCommit,
    existing_installation_ids: &HashMap<String, Vec<Vec<u8>>>,
) -> (Vec<MemberProto>, Vec<MemberProto>) {
    let new_installations: HashMap<String, MemberProto> = staged_commit
        .add_proposals()
        .filter_map(extract_identity_from_add)
        .fold(HashMap::new(), merge_members);

    // Partition the list. If no existing member found, it is a new member. Otherwise it is just new installations
    new_installations
        .into_values()
        .partition(|member| !existing_installation_ids.contains_key(&member.wallet_address))
}

// Get a tuple of (removed_members, removed_installations)
fn get_removed_members(
    staged_commit: &StagedCommit,
    existing_installation_ids: &HashMap<String, Vec<Vec<u8>>>,
    openmls_group: &OpenMlsGroup,
) -> (Vec<MemberProto>, Vec<MemberProto>) {
    let removed_installations: HashMap<String, MemberProto> = staged_commit
        .remove_proposals()
        .filter_map(|proposal| extract_identity_from_remove(proposal, openmls_group))
        .fold(HashMap::new(), merge_members);

    // Separate the fully removed members (where all installation ids were removed in the commit) from partial removals
    removed_installations.into_values().partition(|member| {
        match existing_installation_ids.get(&member.wallet_address) {
            Some(entry) => entry.len() == member.installation_ids.len(),
            None => true,
        }
    })
}

impl<'c, ApiClient> MlsGroup<'c, ApiClient>
where
    ApiClient: XmtpApiClient + XmtpMlsClient,
{
    #[allow(dead_code)]
    pub(crate) fn build_group_membership_change(
        &self,
        staged_commit: &StagedCommit,
        openmls_group: &OpenMlsGroup,
    ) -> Result<GroupMembershipChange, GroupError> {
        // Existing installation IDs keyed by wallet address
        let existing_installation_ids: HashMap<String, Vec<Vec<u8>>> = self
            .members()?
            .into_iter()
            .fold(HashMap::new(), |mut acc, curr| {
                acc.insert(curr.account_address, curr.installation_ids);
                acc
            });

        let (members_added, installations_added) =
            get_new_members(staged_commit, &existing_installation_ids);

        let (members_removed, installations_removed) =
            get_removed_members(staged_commit, &existing_installation_ids, openmls_group);

        Ok(GroupMembershipChange {
            members_added,
            members_removed,
            installations_added,
            installations_removed,
        })
    }
}

#[cfg(test)]
mod tests {
    use openmls::prelude_test::KeyPackage;
    use xmtp_api_grpc::Client as GrpcClient;
    use xmtp_cryptography::utils::generate_local_wallet;

    use crate::{builder::ClientBuilder, Client};

    fn get_key_package(client: &Client<GrpcClient>) -> KeyPackage {
        client
            .identity
            .new_key_package(&client.mls_provider(&mut client.store.conn().unwrap()))
            .unwrap()
    }

    #[tokio::test]
    async fn test_membership_changes() {
        let amal = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        let bola = ClientBuilder::new_test_client(generate_local_wallet().into()).await;
        let bola_key_package = get_key_package(&bola);

        let amal_group = amal.create_group().unwrap();
        let mut amal_conn = amal.store.conn().unwrap();
        let amal_provider = amal.mls_provider(&mut amal_conn);
        let mut mls_group = amal_group.load_mls_group(&amal_provider).unwrap();
        // Create a pending commit to add bola to the group
        mls_group
            .add_members(
                &amal_provider,
                &amal.identity.installation_keys,
                &[bola_key_package],
            )
            .unwrap();

        let mut staged_commit = mls_group.pending_commit().unwrap();

        let message = amal_group
            .build_group_membership_change(staged_commit, &mls_group)
            .unwrap();

        assert_eq!(message.installations_added.len(), 0);
        assert_eq!(message.members_added.len(), 1);
        assert_eq!(
            message.members_added[0].wallet_address,
            bola.account_address()
        );

        // Merge the commit adding bola
        mls_group.merge_pending_commit(&amal_provider).unwrap();
        // Now we are going to remove bola

        let bola_leaf_node = mls_group
            .members()
            .find(|m| {
                m.signature_key
                    .eq(&bola.identity.installation_keys.public())
            })
            .unwrap()
            .index;
        mls_group
            .remove_members(
                &amal_provider,
                &amal.identity.installation_keys,
                &[bola_leaf_node],
            )
            .unwrap();

        staged_commit = mls_group.pending_commit().unwrap();
        let remove_message = amal_group
            .build_group_membership_change(staged_commit, &mls_group)
            .unwrap();

        assert_eq!(remove_message.members_removed.len(), 1);
        assert_eq!(remove_message.installations_removed.len(), 0);
    }
}
