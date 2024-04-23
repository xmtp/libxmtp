use prost::Message;
use xmtp_id::associations::{
    apply_update, get_state, AssociationError, AssociationState, AssociationStateDiff,
    IdentityUpdate,
};
use xmtp_proto::api_client::{XmtpIdentityClient, XmtpMlsClient};

use crate::{
    api::GetIdentityUpdatesV2Filter,
    client::ClientError,
    storage::{db_connection::DbConnection, identity_update::StoredIdentityUpdate},
    Client,
};

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
        let filters = inbox_ids
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
}

#[cfg(test)]
mod tests {
    // TODO once we have identity APIs working
}
