use std::collections::{HashMap, HashSet};

use crate::{
    retry::{Retry, RetryableError},
    retry_async, retryable,
    storage::association_state::StoredAssociationState,
};
use futures::future::try_join_all;
use thiserror::Error;
use xmtp_id::{
    associations::{
        apply_update,
        builder::{SignatureRequest, SignatureRequestBuilder, SignatureRequestError},
        generate_inbox_id, get_state,
        unverified::{
            UnverifiedIdentityUpdate, UnverifiedInstallationKeySignature, UnverifiedSignature,
        },
        AssociationError, AssociationState, AssociationStateDiff, IdentityUpdate, MemberIdentifier,
        SignatureError,
    },
    scw_verifier::SmartContractSignatureVerifier,
};

use crate::{
    api::{ApiClientWrapper, GetIdentityUpdatesV2Filter, InboxUpdate},
    client::ClientError,
    groups::group_membership::{GroupMembership, MembershipDiff},
    storage::{db_connection::DbConnection, identity_update::StoredIdentityUpdate},
    Client, XmtpApi,
};

#[derive(Debug, Error)]
pub enum IdentityUpdateError {
    #[error(transparent)]
    InvalidSignatureRequest(#[from] SignatureRequestError),
}

#[derive(Debug)]
pub struct InstallationDiff {
    pub added_installations: HashSet<Vec<u8>>,
    pub removed_installations: HashSet<Vec<u8>>,
}

#[derive(Debug, Error)]
pub enum InstallationDiffError {
    #[error(transparent)]
    Client(#[from] ClientError),
}

impl RetryableError for InstallationDiffError {
    fn is_retryable(&self) -> bool {
        match self {
            InstallationDiffError::Client(client_error) => retryable!(client_error),
        }
    }
}

impl DbConnection {
    /// Take a list of inbox_id/sequence_id tuples and determine which `inbox_id`s have missing entries
    /// in the local DB
    pub(crate) fn filter_inbox_ids_needing_updates<InboxId: AsRef<str> + ToString>(
        &self,
        filters: Vec<(InboxId, i64)>,
    ) -> Result<Vec<String>, ClientError> {
        let existing_sequence_ids = self.get_latest_sequence_id(
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
}

impl<'a, ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi,
    V: SmartContractSignatureVerifier,
{
    /// Get the association state for all provided `inbox_id`/optional `sequence_id` tuples, using the cache when available
    /// If the association state is not available in the cache, this falls back to reconstructing the association state
    /// from Identity Updates in the network.
    pub async fn batch_get_association_state<InboxId: AsRef<str>>(
        &self,
        conn: &DbConnection,
        identifiers: &[(InboxId, Option<i64>)],
    ) -> Result<Vec<AssociationState>, ClientError> {
        let association_states = try_join_all(
            identifiers
                .iter()
                .map(|(inbox_id, to_sequence_id)| {
                    self.get_association_state(conn, inbox_id, *to_sequence_id)
                })
                .collect::<Vec<_>>(),
        )
        .await?;

        Ok(association_states)
    }

    /// Get the latest association state available on the network for the given `inbox_id`
    pub async fn get_latest_association_state<InboxId: AsRef<str>>(
        &self,
        conn: &DbConnection,
        inbox_id: InboxId,
    ) -> Result<AssociationState, ClientError> {
        load_identity_updates(&self.api_client, conn, vec![inbox_id.as_ref().to_string()]).await?;

        self.get_association_state(conn, inbox_id, None).await
    }

    /// Get the association state for a given inbox_id up to the (and inclusive of) the `to_sequence_id`
    /// If no `to_sequence_id` is provided, use the latest value in the database
    pub async fn get_association_state<InboxId: AsRef<str>>(
        &self,
        conn: &DbConnection,
        inbox_id: InboxId,
        to_sequence_id: Option<i64>,
    ) -> Result<AssociationState, ClientError> {
        let inbox_id = inbox_id.as_ref();
        let updates = conn.get_identity_updates(inbox_id, None, to_sequence_id)?;
        let last_sequence_id = updates
            .last()
            .ok_or::<ClientError>(AssociationError::MissingIdentityUpdate.into())?
            .sequence_id;
        if let Some(to_sequence_id) = to_sequence_id {
            if to_sequence_id != last_sequence_id {
                return Err(AssociationError::MissingIdentityUpdate.into());
            }
        }

        if let Some(association_state) =
            StoredAssociationState::read_from_cache(conn, inbox_id.to_string(), last_sequence_id)?
        {
            return Ok(association_state);
        }

        let unverified_updates = updates
            .into_iter()
            .map(UnverifiedIdentityUpdate::try_from)
            .collect::<Result<Vec<UnverifiedIdentityUpdate>, AssociationError>>()?;
        let updates = verify_updates(unverified_updates, &self.scw_verifier).await?;

        let association_state = get_state(updates)?;

        StoredAssociationState::write_to_cache(
            conn,
            inbox_id.to_string(),
            last_sequence_id,
            association_state.clone(),
        )?;

        Ok(association_state)
    }

    /// Calculate the changes between the `starting_sequence_id` and `ending_sequence_id` for the
    /// provided `inbox_id`
    pub(crate) async fn get_association_state_diff<InboxId: AsRef<str>>(
        &self,
        conn: &DbConnection,
        inbox_id: InboxId,
        starting_sequence_id: Option<i64>,
        ending_sequence_id: Option<i64>,
    ) -> Result<AssociationStateDiff, ClientError> {
        tracing::debug!(
            "Computing diff for {:?} from {:?} to {:?}",
            inbox_id.as_ref(),
            starting_sequence_id,
            ending_sequence_id
        );
        // If no starting sequence ID, get all updates from the beginning of the inbox's history up to the ending sequence ID
        if starting_sequence_id.is_none() {
            return Ok(self
                .get_association_state(conn, inbox_id.as_ref(), ending_sequence_id)
                .await?
                .as_diff());
        }

        // Get the initial state to compare against
        let initial_state = self
            .get_association_state(conn, inbox_id.as_ref(), starting_sequence_id)
            .await?;

        // Get any identity updates that need to be applied
        let incremental_updates =
            conn.get_identity_updates(inbox_id.as_ref(), starting_sequence_id, ending_sequence_id)?;

        let last_sequence_id = incremental_updates.last().map(|update| update.sequence_id);
        if ending_sequence_id.is_some()
            && last_sequence_id.is_some()
            && last_sequence_id != ending_sequence_id
        {
            tracing::error!(
                "Did not find the expected last sequence id. Expected: {:?}, Found: {:?}",
                ending_sequence_id,
                last_sequence_id
            );
            return Err(AssociationError::MissingIdentityUpdate.into());
        }

        let unverified_incremental_updates: Vec<UnverifiedIdentityUpdate> = incremental_updates
            .into_iter()
            .map(|update| update.try_into())
            .collect::<Result<Vec<UnverifiedIdentityUpdate>, AssociationError>>()?;

        let incremental_updates =
            verify_updates(unverified_incremental_updates, &self.scw_verifier).await?;
        let mut final_state = initial_state.clone();
        // Apply each update sequentially, aborting in the case of error
        for update in incremental_updates {
            final_state = apply_update(final_state, update)?;
        }

        tracing::debug!("Final state at {:?}: {:?}", last_sequence_id, final_state);
        if let Some(last_sequence_id) = last_sequence_id {
            StoredAssociationState::write_to_cache(
                conn,
                inbox_id.as_ref().to_string(),
                last_sequence_id,
                final_state.clone(),
            )?;
        }

        Ok(initial_state.diff(&final_state))
    }

    /// Generate a `CreateInbox` signature request for the given wallet address.
    /// If no nonce is provided, use 0
    pub async fn create_inbox(
        &self,
        wallet_address: String,
        maybe_nonce: Option<u64>,
    ) -> Result<SignatureRequest, ClientError> {
        let nonce = maybe_nonce.unwrap_or(0);
        let inbox_id = generate_inbox_id(&wallet_address, &nonce)?;
        let installation_public_key = self.identity().installation_keys.verifying_key();
        let member_identifier: MemberIdentifier = wallet_address.to_lowercase().into();

        let builder = SignatureRequestBuilder::new(inbox_id);
        let mut signature_request = builder
            .create_inbox(member_identifier.clone(), nonce)
            .add_association(
                installation_public_key.as_bytes().to_vec().into(),
                member_identifier,
            )
            .build();

        let sig_bytes = self
            .identity()
            .sign(signature_request.signature_text())?
            .to_vec();
        // We can pre-sign the request with an installation key signature, since we have access to the key
        signature_request
            .add_signature(
                UnverifiedSignature::InstallationKey(UnverifiedInstallationKeySignature::new(
                    sig_bytes,
                    installation_public_key,
                )),
                &self.scw_verifier,
            )
            .await?;

        Ok(signature_request)
    }

    /// Generate a `AssociateWallet` signature request using an existing wallet and a new wallet address
    pub fn associate_wallet(
        &self,
        existing_wallet_address: String,
        new_wallet_address: String,
    ) -> Result<SignatureRequest, ClientError> {
        tracing::info!("Associating new wallet with inbox_id {}", self.inbox_id());
        let inbox_id = self.inbox_id();
        let builder = SignatureRequestBuilder::new(inbox_id);

        Ok(builder
            .add_association(new_wallet_address.into(), existing_wallet_address.into())
            .build())
    }

    /// Revoke the given wallets from the association state for the client's inbox
    pub async fn revoke_wallets(
        &self,
        wallets_to_revoke: Vec<String>,
    ) -> Result<SignatureRequest, ClientError> {
        let inbox_id = self.inbox_id();
        let current_state = retry_async!(
            Retry::default(),
            (async {
                self.get_association_state(&self.store().conn()?, &inbox_id, None)
                    .await
            })
        )?;
        let mut builder = SignatureRequestBuilder::new(inbox_id);

        for wallet in wallets_to_revoke {
            builder = builder.revoke_association(
                current_state.recovery_address().clone().into(),
                wallet.into(),
            )
        }

        Ok(builder.build())
    }

    /// Revoke the given installations from the association state for the client's inbox
    pub async fn revoke_installations(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<SignatureRequest, ClientError> {
        let inbox_id = self.inbox_id();

        let current_state = retry_async!(
            Retry::default(),
            (async {
                self.get_association_state(&self.store().conn()?, &inbox_id, None)
                    .await
            })
        )?;

        let mut builder = SignatureRequestBuilder::new(inbox_id);

        for installation_id in installation_ids {
            builder = builder.revoke_association(
                current_state.recovery_address().clone().into(),
                installation_id.into(),
            )
        }

        Ok(builder.build())
    }

    /**
     * Apply a signature request to the client's inbox by publishing the identity update to the network.
     *
     * This will error if the signature request is missing signatures, if the signatures are invalid,
     * if the update fails other verifications, or if the update fails to be published to the network.
     **/
    pub async fn apply_signature_request(
        &self,
        signature_request: SignatureRequest,
    ) -> Result<(), ClientError> {
        let inbox_id = signature_request.inbox_id();
        // If the signature request isn't completed, this will error
        let identity_update = signature_request
            .build_identity_update()
            .map_err(IdentityUpdateError::from)?;

        identity_update.to_verified(self.scw_verifier()).await?;

        // We don't need to validate the update, since the server will do this for us
        self.api_client
            .publish_identity_update(identity_update)
            .await?;

        // Load the identity updates for the inbox so that we have a record in our DB
        retry_async!(
            Retry::default(),
            (async {
                load_identity_updates(
                    &self.api_client,
                    &self.store().conn()?,
                    vec![inbox_id.clone()],
                )
                .await
            })
        )?;

        Ok(())
    }

    /// Given two group memberships and the diff, get the list of installations that were added or removed
    /// between the two membership states.
    pub async fn get_installation_diff(
        &self,
        conn: &DbConnection,
        old_group_membership: &GroupMembership,
        new_group_membership: &GroupMembership,
        membership_diff: &MembershipDiff<'_>,
    ) -> Result<InstallationDiff, InstallationDiffError> {
        tracing::info!(
            "Getting installation diff. Old: {:?}. New {:?}",
            old_group_membership,
            new_group_membership
        );
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

        load_identity_updates(
            &self.api_client,
            conn,
            conn.filter_inbox_ids_needing_updates(filters)?,
        )
        .await?;

        let mut added_installations: HashSet<Vec<u8>> = HashSet::new();
        let mut removed_installations: HashSet<Vec<u8>> = HashSet::new();

        // TODO: Do all of this in parallel
        for inbox_id in added_and_updated_members {
            let starting_sequence_id = match old_group_membership.get(inbox_id) {
                Some(0) => None,
                Some(i) => Some(*i as i64),
                None => None,
            };
            let state_diff = self
                .get_association_state_diff(
                    conn,
                    inbox_id,
                    starting_sequence_id,
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

/// For the given list of `inbox_id`s get all updates from the network that are newer than the last known `sequence_id`,
/// write them in the db, and return the updates
#[tracing::instrument(level = "trace", skip_all)]
pub async fn load_identity_updates<ApiClient: XmtpApi>(
    api_client: &ApiClientWrapper<ApiClient>,
    conn: &DbConnection,
    inbox_ids: Vec<String>,
) -> Result<HashMap<String, Vec<InboxUpdate>>, ClientError> {
    if inbox_ids.is_empty() {
        return Ok(HashMap::new());
    }
    tracing::debug!("Fetching identity updates for: {:?}", inbox_ids);

    let existing_sequence_ids = conn.get_latest_sequence_id(&inbox_ids)?;
    let filters: Vec<GetIdentityUpdatesV2Filter> = inbox_ids
        .into_iter()
        .map(|inbox_id| GetIdentityUpdatesV2Filter {
            sequence_id: existing_sequence_ids.get(&inbox_id).map(|i| *i as u64),
            inbox_id,
        })
        .collect();

    let updates = api_client.get_identity_updates_v2(filters).await?;

    let to_store = updates
        .iter()
        .flat_map(|(inbox_id, updates)| {
            updates.iter().map(|update| StoredIdentityUpdate {
                inbox_id: inbox_id.clone(),
                sequence_id: update.sequence_id as i64,
                server_timestamp_ns: update.server_timestamp_ns as i64,
                payload: update.update.clone().into(),
            })
        })
        .collect::<Vec<StoredIdentityUpdate>>();

    conn.insert_or_ignore_identity_updates(&to_store)?;
    Ok(updates)
}

/// Convert a list of unverified updates to verified updates using the given smart contract verifier
async fn verify_updates(
    updates: Vec<UnverifiedIdentityUpdate>,
    scw_verifier: impl SmartContractSignatureVerifier,
) -> Result<Vec<IdentityUpdate>, SignatureError> {
    try_join_all(
        updates
            .iter()
            .map(|update| update.to_verified(&scw_verifier)),
    )
    .await
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_id::{
        associations::{
            builder::SignatureRequest, test_utils::add_wallet_signature, AssociationState,
            MemberIdentifier,
        },
        scw_verifier::SmartContractSignatureVerifier,
        InboxOwner,
    };

    use crate::{
        builder::ClientBuilder,
        groups::group_membership::GroupMembership,
        storage::{db_connection::DbConnection, identity_update::StoredIdentityUpdate},
        utils::test::{rand_vec, FullXmtpClient},
        Client, XmtpApi,
    };

    use super::load_identity_updates;

    async fn get_association_state<ApiClient, Verifier>(
        client: &Client<ApiClient, Verifier>,
        inbox_id: String,
    ) -> AssociationState
    where
        ApiClient: XmtpApi,
        Verifier: SmartContractSignatureVerifier,
    {
        let conn = client.store().conn().unwrap();
        load_identity_updates(&client.api_client, &conn, vec![inbox_id.clone()])
            .await
            .unwrap();

        client
            .get_association_state(&conn, inbox_id, None)
            .await
            .unwrap()
    }

    fn insert_identity_update(conn: &DbConnection, inbox_id: &str, sequence_id: i64) {
        let identity_update =
            StoredIdentityUpdate::new(inbox_id.to_string(), sequence_id, 0, rand_vec());

        conn.insert_or_ignore_identity_updates(&[identity_update])
            .expect("insert should succeed");
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn create_inbox_round_trip() {
        let wallet = generate_local_wallet();
        let wallet_address = wallet.get_address();
        let client = ClientBuilder::new_test_client(&wallet).await;

        let mut signature_request: SignatureRequest = client
            .create_inbox(wallet_address.clone(), None)
            .await
            .unwrap();
        let inbox_id = signature_request.inbox_id();

        add_wallet_signature(&mut signature_request, &wallet).await;

        client
            .apply_signature_request(signature_request)
            .await
            .unwrap();

        let association_state = get_association_state(&client, inbox_id.clone()).await;

        assert_eq!(association_state.members().len(), 2);
        assert_eq!(association_state.recovery_address(), &wallet_address);
        assert!(association_state.get(&wallet_address.into()).is_some())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn add_association() {
        let wallet = generate_local_wallet();
        let wallet_2 = generate_local_wallet();
        let wallet_address = wallet.get_address();
        let wallet_2_address = wallet_2.get_address();
        let client = ClientBuilder::new_test_client(&wallet).await;

        let mut add_association_request = client
            .associate_wallet(wallet_address.clone(), wallet_2_address.clone())
            .unwrap();

        add_wallet_signature(&mut add_association_request, &wallet).await;
        add_wallet_signature(&mut add_association_request, &wallet_2).await;

        client
            .apply_signature_request(add_association_request)
            .await
            .unwrap();

        let association_state = get_association_state(&client, client.inbox_id()).await;

        let members =
            association_state.members_by_parent(&MemberIdentifier::Address(wallet_address.clone()));
        // Those members should have timestamps
        for member in members {
            assert!(member.client_timestamp_ns.is_some());
        }

        assert_eq!(association_state.members().len(), 3);
        assert_eq!(association_state.recovery_address(), &wallet_address);
        assert!(association_state.get(&wallet_2_address.into()).is_some());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg(not(target_arch = "wasm32"))]
    fn cache_association_state() {
        use crate::{assert_logged, utils::test::traced_test};
        traced_test(|| async {
            let wallet = generate_local_wallet();
            let wallet_2 = generate_local_wallet();
            let wallet_address = wallet.get_address();
            let wallet_2_address = wallet_2.get_address();
            let client = ClientBuilder::new_test_client(&wallet).await;
            let inbox_id = client.inbox_id();

            get_association_state(&client, inbox_id.clone()).await;

            assert_logged!("Loaded association", 0);
            assert_logged!("Wrote association", 1);

            let association_state = get_association_state(&client, inbox_id.clone()).await;

            assert_eq!(association_state.members().len(), 2);
            assert_eq!(association_state.recovery_address(), &wallet_address);
            assert!(association_state
                .get(&wallet_address.clone().into())
                .is_some());

            assert_logged!("Loaded association", 1);
            assert_logged!("Wrote association", 1);

            let mut add_association_request = client
                .associate_wallet(wallet_address.clone(), wallet_2_address.clone())
                .unwrap();

            add_wallet_signature(&mut add_association_request, &wallet).await;
            add_wallet_signature(&mut add_association_request, &wallet_2).await;

            client
                .apply_signature_request(add_association_request)
                .await
                .unwrap();

            get_association_state(&client, inbox_id.clone()).await;

            assert_logged!("Loaded association", 1);
            assert_logged!("Wrote association", 2);

            let association_state = get_association_state(&client, inbox_id.clone()).await;

            assert_logged!("Loaded association", 2);
            assert_logged!("Wrote association", 2);

            assert_eq!(association_state.members().len(), 3);
            assert_eq!(association_state.recovery_address(), &wallet_address);
            assert!(association_state.get(&wallet_2_address.into()).is_some());
        });
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn load_identity_updates_if_needed() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        let conn = client.store().conn().unwrap();

        insert_identity_update(&conn, "inbox_1", 1);
        insert_identity_update(&conn, "inbox_2", 2);
        insert_identity_update(&conn, "inbox_3", 3);

        let filtered =
            // Inbox 1 is requesting an inbox ID higher than what is in the DB. Inbox 2 is requesting one that matches the DB.
            // Inbox 3 is requesting one lower than what is in the DB
            conn.filter_inbox_ids_needing_updates(vec![("inbox_1", 3), ("inbox_2", 2), ("inbox_3", 2)]);
        assert_eq!(filtered.unwrap(), vec!["inbox_1"]);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
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

            add_wallet_signature(&mut signature_request, &wallet).await;
            client
                .apply_signature_request(signature_request)
                .await
                .unwrap();
            let new_wallet = generate_local_wallet();
            let mut add_association_request = client
                .associate_wallet(wallet.get_address(), new_wallet.get_address())
                .unwrap();

            add_wallet_signature(&mut add_association_request, &wallet).await;
            add_wallet_signature(&mut add_association_request, &new_wallet).await;

            client
                .apply_signature_request(add_association_request)
                .await
                .unwrap();
        }

        // Create a new client to test group operations with
        let other_client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let other_conn = other_client.store().conn().unwrap();
        // Load all the identity updates for the new inboxes
        load_identity_updates(&other_client.api_client, &other_conn, inbox_ids.clone())
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
            *latest_sequence_ids.get(&inbox_ids[1]).unwrap() as u64,
        );

        let mut new_group_membership = original_group_membership.clone();
        // Update the first inbox to have a higher sequence ID, but no new installations
        new_group_membership.add(
            inbox_ids[0].clone(),
            *latest_sequence_ids.get(&inbox_ids[0]).unwrap() as u64,
        );
        new_group_membership.add(
            inbox_ids[2].clone(),
            *latest_sequence_ids.get(&inbox_ids[2]).unwrap() as u64,
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

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    pub async fn revoke_wallet() {
        let recovery_wallet = generate_local_wallet();
        let second_wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&recovery_wallet).await;

        let mut add_wallet_signature_request = client
            .associate_wallet(recovery_wallet.get_address(), second_wallet.get_address())
            .unwrap();

        add_wallet_signature(&mut add_wallet_signature_request, &recovery_wallet).await;
        add_wallet_signature(&mut add_wallet_signature_request, &second_wallet).await;

        client
            .apply_signature_request(add_wallet_signature_request)
            .await
            .unwrap();

        let association_state_after_add = get_association_state(&client, client.inbox_id()).await;
        assert_eq!(association_state_after_add.account_addresses().len(), 2);

        // Make sure the inbox ID is correctly registered
        let inbox_ids = client
            .api_client
            .get_inbox_ids(vec![second_wallet.get_address()])
            .await
            .unwrap();
        assert_eq!(inbox_ids.len(), 1);

        // Now revoke the second wallet

        let mut revoke_signature_request = client
            .revoke_wallets(vec![second_wallet.get_address()])
            .await
            .unwrap();
        add_wallet_signature(&mut revoke_signature_request, &recovery_wallet).await;

        client
            .apply_signature_request(revoke_signature_request)
            .await
            .unwrap();

        // Make sure that the association state has removed the second wallet
        let association_state_after_revoke =
            get_association_state(&client, client.inbox_id()).await;
        assert_eq!(association_state_after_revoke.account_addresses().len(), 1);

        // Make sure the inbox ID is correctly unregistered
        let inbox_ids = client
            .api_client
            .get_inbox_ids(vec![second_wallet.get_address()])
            .await
            .unwrap();
        assert_eq!(inbox_ids.len(), 0);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    pub async fn revoke_installation() {
        let wallet = generate_local_wallet();
        let client1: FullXmtpClient = ClientBuilder::new_test_client(&wallet).await;
        let client2: FullXmtpClient = ClientBuilder::new_test_client(&wallet).await;

        let association_state = get_association_state(&client1, client1.inbox_id()).await;
        // Ensure there are two installations on the inbox
        assert_eq!(association_state.installation_ids().len(), 2);

        // Now revoke the second client
        let mut revoke_installation_request = client1
            .revoke_installations(vec![client2.installation_public_key()])
            .await
            .unwrap();
        add_wallet_signature(&mut revoke_installation_request, &wallet).await;
        client1
            .apply_signature_request(revoke_installation_request)
            .await
            .unwrap();

        // Make sure there is only one installation on the inbox
        let association_state = get_association_state(&client1, client1.inbox_id()).await;
        assert_eq!(association_state.installation_ids().len(), 1);
    }
}
