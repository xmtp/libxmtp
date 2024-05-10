use std::collections::HashMap;

use openmls::{credentials::BasicCredential, group::MlsGroup as OpenMlsGroup};

use xmtp_proto::api_client::{XmtpIdentityClient, XmtpMlsClient};

use super::{GroupError, MlsGroup};

use crate::{identity::xmtp_id::Identity, xmtp_openmls_provider::XmtpOpenMlsProvider};

#[derive(Debug, Clone)]
pub struct GroupMember {
    pub account_address: String,
    pub installation_ids: Vec<Vec<u8>>,
}

impl<'c, ApiClient> MlsGroup<'c, ApiClient>
where
    ApiClient: XmtpMlsClient + XmtpIdentityClient,
{
    // Load the member list for the group from the DB, merging together multiple installations into a single entry
    pub fn members(&self) -> Result<Vec<GroupMember>, GroupError> {
        let conn = self.client.store.conn()?;
        let provider = self.client.mls_provider(&conn);
        self.members_with_provider(&provider)
    }

    pub fn members_with_provider(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<Vec<GroupMember>, GroupError> {
        let openmls_group = self.load_mls_group(provider)?;
        aggregate_member_list(&openmls_group)
    }
}

pub fn aggregate_member_list(openmls_group: &OpenMlsGroup) -> Result<Vec<GroupMember>, GroupError> {
    let member_map: HashMap<String, GroupMember> = openmls_group
        .members()
        .filter_map(|member| {
            let basic_credential = BasicCredential::try_from(&member.credential).ok()?;
            Identity::get_validated_account_address(
                basic_credential.identity(),
                &member.signature_key,
            )
            .ok()
            .map(|account_address| (account_address, member.signature_key.clone()))
        })
        .fold(
            HashMap::new(),
            |mut acc, (account_address, signature_key)| {
                acc.entry(account_address.clone())
                    .and_modify(|e| e.installation_ids.push(signature_key.clone()))
                    .or_insert(GroupMember {
                        account_address,
                        installation_ids: vec![signature_key],
                    });
                acc
            },
        );

    Ok(member_map.into_values().collect())
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
        // // Add both of Bola's installations to the group
        // group
        //     .add_members_by_installation_id(vec![
        //         bola_a.installation_public_key(),
        //         bola_b.installation_public_key(),
        //     ])
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
