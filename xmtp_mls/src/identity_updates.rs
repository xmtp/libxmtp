use std::collections::HashSet;

use prost::Message;
use thiserror::Error;
use xmtp_id::associations::{
    apply_update,
    builder::{SignatureRequest, SignatureRequestBuilder, SignatureRequestError},
    generate_inbox_id, get_state, AssociationError, AssociationState, AssociationStateDiff,
    IdentityUpdate, InstallationKeySignature, MemberIdentifier,
};
use xmtp_proto::api_client::{XmtpIdentityClient, XmtpMlsClient};

use crate::{
    api::GetIdentityUpdatesV2Filter,
    client::ClientError,
    groups::group_membership::{GroupMembership, MembershipDiff},
    storage::{db_connection::DbConnection, identity_update::StoredIdentityUpdate},
    Client,
};

#[derive(Debug, Error)]
pub enum IdentityUpdateError {
    #[error(transparent)]
    InvalidSignatureRequest(#[from] SignatureRequestError),
}

pub struct InstallationDiff {
    pub added_installations: HashSet<Vec<u8>>,
    pub removed_installations: HashSet<Vec<u8>>,
}

#[derive(Debug, Error)]
pub enum InstallationDiffError {
    #[error(transparent)]
    Client(#[from] ClientError),
}

impl<'a, ApiClient> Client<ApiClient>
where
    ApiClient: XmtpMlsClient + XmtpIdentityClient,
{
    /// For the given list of `inbox_id`s get all updates from the network that are newer than the last known `sequence_id``
    pub async fn load_identity_updates(
        &self,
        conn: &'a DbConnection<'a>,
        inbox_ids: Vec<String>,
    ) -> Result<(), ClientError> {
        if inbox_ids.is_empty() {
            return Ok(());
        }

        let existing_sequence_ids = conn.get_latest_sequence_id(&inbox_ids)?;
        let filters: Vec<GetIdentityUpdatesV2Filter> = inbox_ids
            .into_iter()
            .map(|inbox_id| GetIdentityUpdatesV2Filter {
                sequence_id: existing_sequence_ids
                    .get(&inbox_id)
                    .cloned()
                    .map(|i| i as u64),
                inbox_id,
            })
            .collect();

        let updates = self.api_client.get_identity_updates_v2(filters).await?;

        let to_store = updates
            .into_iter()
            .flat_map(|(inbox_id, updates)| {
                updates.into_iter().map(move |update| StoredIdentityUpdate {
                    inbox_id: inbox_id.clone(),
                    sequence_id: update.sequence_id as i64,
                    server_timestamp_ns: update.server_timestamp_ns as i64,
                    payload: update.update.to_proto().encode_to_vec(),
                })
            })
            .collect::<Vec<StoredIdentityUpdate>>();

        Ok(conn.insert_or_ignore_identity_updates(&to_store)?)
    }

    /// Take a list of inbox_id/sequence_id tuples and determine which `inbox_id`s have missing entries
    /// in the local DB
    fn filter_inbox_ids_needing_updates<InboxId: AsRef<str> + ToString>(
        &self,
        conn: &'a DbConnection<'a>,
        filters: Vec<(InboxId, i64)>,
    ) -> Result<Vec<String>, ClientError> {
        let existing_sequence_ids = conn.get_latest_sequence_id(
            &filters
                .iter()
                .map(|f| f.0.to_string())
                .collect::<Vec<String>>(),
        )?;

        let needs_update = filters
            .iter()
            .filter_map(|filter| {
                let existing_sequence_id = existing_sequence_ids.get(filter.0.as_ref());
                if let Some(sequence_id) = existing_sequence_id {
                    if sequence_id.ge(&filter.1) {
                        return None;
                    }
                }

                Some(filter.0.to_string())
            })
            .collect::<Vec<String>>();

        Ok(needs_update)
    }

    pub async fn get_association_state<InboxId: AsRef<str>>(
        &self,
        conn: &'a DbConnection<'a>,
        inbox_id: InboxId,
        to_sequence_id: Option<i64>,
    ) -> Result<AssociationState, ClientError> {
        // TODO: Check against a local cache before talking to the network

        let updates = conn.get_identity_updates(inbox_id, None, to_sequence_id)?;
        let last_update = updates.last();
        if last_update.is_none() {
            return Err(AssociationError::MissingIdentityUpdate.into());
        }
        if let Some(sequence_id) = to_sequence_id {
            if last_update
                .expect("already checked")
                .sequence_id
                .ne(&sequence_id)
            {
                return Err(AssociationError::MissingIdentityUpdate.into());
            }
        }
        let updates = updates
            .into_iter()
            .map(IdentityUpdate::try_from)
            .collect::<Result<Vec<IdentityUpdate>, AssociationError>>()?;

        Ok(get_state(updates).await?)
    }

    pub(crate) async fn get_association_state_diff<InboxId: AsRef<str>>(
        &self,
        conn: &'a DbConnection<'a>,
        inbox_id: InboxId,
        starting_sequence_id: Option<i64>,
        ending_sequence_id: Option<i64>,
    ) -> Result<AssociationStateDiff, ClientError> {
        if starting_sequence_id.is_none() {
            return Ok(self
                .get_association_state(conn, inbox_id.as_ref(), ending_sequence_id)
                .await?
                .as_diff());
        }

        let initial_state = self
            .get_association_state(conn, inbox_id.as_ref(), starting_sequence_id)
            .await?;

        let incremental_updates = conn
            .get_identity_updates(inbox_id, starting_sequence_id, ending_sequence_id)?
            .into_iter()
            .map(|update| update.try_into())
            .collect::<Result<Vec<IdentityUpdate>, AssociationError>>()?;

        let mut final_state = initial_state.clone();
        for update in incremental_updates {
            final_state = apply_update(final_state, update).await?;
        }

        Ok(initial_state.diff(&final_state))
    }

    pub async fn create_inbox(
        &self,
        wallet_address: String,
        maybe_nonce: Option<u64>,
    ) -> Result<SignatureRequest, ClientError> {
        let nonce = maybe_nonce.unwrap_or(0);
        let inbox_id = generate_inbox_id(&wallet_address, &nonce);
        let installation_public_key = self.identity.installation_keys.public();
        let member_identifier: MemberIdentifier = wallet_address.into();

        let builder = SignatureRequestBuilder::new(inbox_id);
        let mut signature_request = builder
            .create_inbox(member_identifier.clone(), nonce)
            .add_association(installation_public_key.to_vec().into(), member_identifier)
            .build();

        // We can pre-sign the request with an installation key signature, since we have access to the key
        signature_request
            .add_signature(Box::new(InstallationKeySignature::new(
                signature_request.signature_text(),
                self.identity.sign(signature_request.signature_text())?,
                self.installation_public_key(),
            )))
            .await?;

        Ok(signature_request)
    }

    pub fn associate_wallet(
        &self,
        // TODO: Replace this argument with a value stored on the client
        inbox_id: String,
        existing_wallet_address: String,
        new_wallet_address: String,
    ) -> Result<SignatureRequest, ClientError> {
        let builder = SignatureRequestBuilder::new(inbox_id);

        Ok(builder
            .add_association(new_wallet_address.into(), existing_wallet_address.into())
            .build())
    }

    pub async fn revoke_wallet(
        &self,
        inbox_id: String,
        wallet_to_revoke: String,
    ) -> Result<SignatureRequest, ClientError> {
        let current_state = self
            .get_association_state(&self.store.conn()?, &inbox_id, None)
            .await?;
        let builder = SignatureRequestBuilder::new(inbox_id);

        Ok(builder
            .revoke_association(
                current_state.recovery_address().clone().into(),
                wallet_to_revoke.into(),
            )
            .build())
    }

    pub async fn apply_signature_request(
        &self,
        signature_request: SignatureRequest,
    ) -> Result<(), ClientError> {
        // If the signature request isn't completed, this will error
        let identity_update = signature_request
            .build_identity_update()
            .map_err(IdentityUpdateError::from)?;

        // We don't need to validate the update, since the server will do this for us
        self.api_client
            .publish_identity_update(identity_update)
            .await?;

        Ok(())
    }

    /// Given two group memberships and the diff, get the list of installations that were added or removed
    /// between the two membership states.
    pub async fn get_installation_diff<'diff>(
        &self,
        conn: &'a DbConnection<'a>,
        old_group_membership: &GroupMembership,
        new_group_membership: &GroupMembership,
        membership_diff: &MembershipDiff<'diff>,
    ) -> Result<InstallationDiff, InstallationDiffError> {
        let added_and_updated_members = membership_diff
            .added_inboxes
            .iter()
            .chain(membership_diff.updated_inboxes.iter())
            .cloned();

        let filters = added_and_updated_members
            .clone()
            .map(|i| {
                (
                    i,
                    new_group_membership.get(i).map(|i| *i as i64).unwrap_or(0),
                )
            })
            .collect::<Vec<(&String, i64)>>();

        self.load_identity_updates(conn, self.filter_inbox_ids_needing_updates(conn, filters)?)
            .await?;

        let mut added_installations: HashSet<Vec<u8>> = HashSet::new();
        let mut removed_installations: HashSet<Vec<u8>> = HashSet::new();

        // TODO: Do all of this in parallel
        for inbox_id in added_and_updated_members {
            let state_diff = self
                .get_association_state_diff(
                    conn,
                    inbox_id,
                    old_group_membership.get(inbox_id).map(|i| *i as i64),
                    new_group_membership.get(inbox_id).map(|i| *i as i64),
                )
                .await?;

            added_installations.extend(state_diff.new_installations());
            removed_installations.extend(state_diff.removed_installations());
        }

        for inbox_id in membership_diff.removed_inboxes.iter() {
            let state_diff = self
                .get_association_state(
                    conn,
                    inbox_id,
                    old_group_membership.get(inbox_id).map(|i| *i as i64),
                )
                .await?
                .as_diff();

            // In the case of a removed member, get all the "new installations" from the diff and add them to the list of removed installations
            removed_installations.extend(state_diff.new_installations());
        }

        Ok(InstallationDiff {
            added_installations,
            removed_installations,
        })
    }
}

#[cfg(test)]
mod tests {
    use ethers::signers::LocalWallet;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_id::{
        associations::{builder::SignatureRequest, AssociationState, RecoverableEcdsaSignature},
        InboxOwner,
    };
    use xmtp_proto::api_client::{XmtpIdentityClient, XmtpMlsClient};

    use crate::{
        builder::ClientBuilder,
        groups::group_membership::GroupMembership,
        storage::{db_connection::DbConnection, identity_update::StoredIdentityUpdate},
        utils::test::rand_vec,
        Client,
    };

    async fn sign_with_wallet(wallet: &LocalWallet, signature_request: &mut SignatureRequest) {
        let wallet_signature: Vec<u8> = wallet
            .sign(signature_request.signature_text().as_str())
            .unwrap()
            .into();

        signature_request
            .add_signature(Box::new(RecoverableEcdsaSignature::new(
                signature_request.signature_text(),
                wallet_signature,
            )))
            .await
            .unwrap();
    }

    async fn get_association_state<ApiClient: XmtpIdentityClient + XmtpMlsClient>(
        client: &Client<ApiClient>,
        inbox_id: String,
    ) -> AssociationState {
        let conn = client.store.conn().unwrap();
        client
            .load_identity_updates(&conn, vec![inbox_id.clone()])
            .await
            .unwrap();

        client
            .get_association_state(&conn, inbox_id, None)
            .await
            .unwrap()
    }

    fn insert_identity_update(conn: &DbConnection<'_>, inbox_id: &str, sequence_id: i64) -> () {
        let identity_update =
            StoredIdentityUpdate::new(inbox_id.to_string(), sequence_id, 0, rand_vec());

        conn.insert_or_ignore_identity_updates(&[identity_update])
            .expect("insert should succeed");
    }

    #[tokio::test]
    async fn create_inbox_round_trip() {
        let wallet = generate_local_wallet();
        let wallet_address = wallet.get_address();
        let client = ClientBuilder::new_test_client(&wallet).await;

        let mut signature_request: SignatureRequest = client
            .create_inbox(wallet_address.clone(), None)
            .await
            .unwrap();
        let inbox_id = signature_request.inbox_id();

        sign_with_wallet(&wallet, &mut signature_request).await;

        client
            .apply_signature_request(signature_request)
            .await
            .unwrap();

        let association_state = get_association_state(&client, inbox_id.clone()).await;

        assert_eq!(association_state.members().len(), 2);
        assert_eq!(association_state.recovery_address(), &wallet_address);
        assert!(association_state.get(&wallet_address.into()).is_some())
    }

    #[tokio::test]
    async fn add_association() {
        let wallet = generate_local_wallet();
        let wallet_2 = generate_local_wallet();
        let wallet_address = wallet.get_address();
        let wallet_2_address = wallet_2.get_address();
        let client = ClientBuilder::new_test_client(&wallet).await;

        let mut signature_request: SignatureRequest = client
            .create_inbox(wallet_address.clone(), None)
            .await
            .unwrap();
        let inbox_id = signature_request.inbox_id();

        sign_with_wallet(&wallet, &mut signature_request).await;

        client
            .apply_signature_request(signature_request)
            .await
            .unwrap();

        let mut add_association_request = client
            .associate_wallet(
                inbox_id.clone(),
                wallet_address.clone(),
                wallet_2_address.clone(),
            )
            .unwrap();

        sign_with_wallet(&wallet, &mut add_association_request).await;
        sign_with_wallet(&wallet_2, &mut add_association_request).await;

        client
            .apply_signature_request(add_association_request)
            .await
            .unwrap();

        let association_state = get_association_state(&client, inbox_id.clone()).await;

        assert_eq!(association_state.members().len(), 3);
        assert_eq!(association_state.recovery_address(), &wallet_address);
        assert!(association_state.get(&wallet_2_address.into()).is_some());
    }

    #[tokio::test]
    async fn load_identity_updates_if_needed() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        let conn = client.store.conn().unwrap();

        insert_identity_update(&conn, "inbox_1", 1);
        insert_identity_update(&conn, "inbox_2", 2);
        insert_identity_update(&conn, "inbox_3", 3);

        let filtered =
            // Inbox 1 is requesting an inbox ID higher than what is in the DB. Inbox 2 is requesting one that matches the DB.
            // Inbox 3 is requesting one lower than what is in the DB
            client.filter_inbox_ids_needing_updates(&conn, vec![("inbox_1", 3), ("inbox_2", 2), ("inbox_3", 2)]);
        assert_eq!(filtered.unwrap(), vec!["inbox_1"]);
    }

    #[tokio::test]
    async fn get_installation_diff() {
        let wallet_1 = generate_local_wallet();
        let wallet_2 = generate_local_wallet();
        let wallet_3 = generate_local_wallet();

        let client_1 = ClientBuilder::new_test_client(&wallet_1).await;
        let client_2 = ClientBuilder::new_test_client(&wallet_2).await;
        let client_3 = ClientBuilder::new_test_client(&wallet_3).await;

        let client_2_installation_key = client_2.installation_public_key();
        let client_3_installation_key = client_3.installation_public_key();

        let mut inbox_ids: Vec<String> = vec![];

        // Create an inbox with 2 history items for each client
        for (client, wallet) in vec![
            (client_1, wallet_1),
            (client_2, wallet_2),
            (client_3, wallet_3),
        ] {
            let mut signature_request: SignatureRequest = client
                .create_inbox(wallet.get_address(), None)
                .await
                .unwrap();
            let inbox_id = signature_request.inbox_id();
            inbox_ids.push(inbox_id.clone());

            sign_with_wallet(&wallet, &mut signature_request).await;
            client
                .apply_signature_request(signature_request)
                .await
                .unwrap();
            let new_wallet = generate_local_wallet();
            let mut add_association_request = client
                .associate_wallet(inbox_id, wallet.get_address(), new_wallet.get_address())
                .unwrap();

            sign_with_wallet(&wallet, &mut add_association_request).await;
            sign_with_wallet(&new_wallet, &mut add_association_request).await;

            client
                .apply_signature_request(add_association_request)
                .await
                .unwrap();
        }

        // Create a new client to test group operations with
        let other_client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let other_conn = other_client.store.conn().unwrap();
        // Load all the identity updates for the new inboxes
        other_client
            .load_identity_updates(&other_conn, inbox_ids.clone())
            .await
            .expect("load should succeed");

        // Get the latest sequence IDs so we can construct the updates
        let latest_sequence_ids = other_conn
            .get_latest_sequence_id(&inbox_ids.clone())
            .unwrap();

        let inbox_1_first_sequence_id = other_conn
            .get_identity_updates(inbox_ids[0].clone(), None, None)
            .unwrap()
            .first()
            .unwrap()
            .sequence_id;

        let mut original_group_membership = GroupMembership::new();
        original_group_membership.add(inbox_ids[0].clone(), inbox_1_first_sequence_id as u64);
        original_group_membership.add(
            inbox_ids[1].clone(),
            latest_sequence_ids.get(&inbox_ids[1]).unwrap().clone() as u64,
        );

        let mut new_group_membership = original_group_membership.clone();
        // Update the first inbox to have a higher sequence ID, but no new installations
        new_group_membership.add(
            inbox_ids[0].clone(),
            latest_sequence_ids.get(&inbox_ids[0]).unwrap().clone() as u64,
        );
        new_group_membership.add(
            inbox_ids[2].clone(),
            latest_sequence_ids.get(&inbox_ids[2]).unwrap().clone() as u64,
        );
        new_group_membership.remove(&inbox_ids[1]);

        let membership_diff = original_group_membership.diff(&new_group_membership);

        let installation_diff = other_client
            .get_installation_diff(
                &other_conn,
                &original_group_membership,
                &new_group_membership,
                &membership_diff,
            )
            .await
            .unwrap();

        assert_eq!(installation_diff.added_installations.len(), 1);
        assert!(installation_diff
            .added_installations
            .contains(&client_3_installation_key),);
        assert_eq!(installation_diff.removed_installations.len(), 1);
        assert!(installation_diff
            .removed_installations
            .contains(&client_2_installation_key));
    }
}
