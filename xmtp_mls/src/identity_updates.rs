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
    storage::{db_connection::DbConnection, identity_update::StoredIdentityUpdate},
    Client,
};

#[derive(Debug, Error)]
pub enum IdentityUpdateError {
    #[error(transparent)]
    InvalidSignatureRequest(#[from] SignatureRequestError),
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

    pub async fn get_association_state<InboxId: AsRef<str>>(
        &self,
        conn: &'a DbConnection<'a>,
        inbox_id: InboxId,
        to_sequence_id: Option<i64>,
    ) -> Result<AssociationState, ClientError> {
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

    pub async fn get_association_state_diff<InboxId: AsRef<str>>(
        &self,
        conn: &'a DbConnection<'a>,
        inbox_id: String,
        starting_sequence_id: Option<i64>,
        ending_sequence_id: Option<i64>,
    ) -> Result<AssociationStateDiff, ClientError> {
        let initial_state = self
            .get_association_state(conn, &inbox_id, starting_sequence_id)
            .await?;
        if starting_sequence_id.is_none() {
            return Ok(initial_state.as_diff());
        }

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
                // TODO: Move this to a method on the new identity
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

    pub async fn get_inbox_id(
        &self,
        wallet_address: String,
    ) -> Result<Option<String>, ClientError> {
        let inbox_map = self
            .api_client
            .get_inbox_ids(vec![wallet_address.clone()])
            .await?;

        Ok(inbox_map.get(&wallet_address).cloned().unwrap_or(None))
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

    use crate::{builder::ClientBuilder, Client};

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
}
