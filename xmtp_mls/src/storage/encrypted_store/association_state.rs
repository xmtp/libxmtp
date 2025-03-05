use diesel::prelude::*;
use prost::Message;
use xmtp_id::{associations::AssociationState, InboxId};
use xmtp_proto::xmtp::identity::associations::AssociationState as AssociationStateProto;
use xmtp_proto::ConversionError;

use super::{
    schema::association_state::{self, dsl},
    DbConnection,
};
use crate::{impl_fetch, impl_store_or_ignore, storage::StorageError, Fetch, StoreOrIgnore};

/// StoredIdentityUpdate holds a serialized IdentityUpdate record
#[derive(Insertable, Identifiable, Queryable, Debug, Clone, PartialEq, Eq)]
#[diesel(table_name = association_state)]
#[diesel(primary_key(inbox_id, sequence_id))]
pub struct StoredAssociationState {
    pub inbox_id: String,
    pub sequence_id: i64,
    pub state: Vec<u8>,
}
impl_fetch!(StoredAssociationState, association_state, (String, i64));
impl_store_or_ignore!(StoredAssociationState, association_state);

impl TryFrom<StoredAssociationState> for AssociationState {
    type Error = ConversionError;

    fn try_from(stored_state: StoredAssociationState) -> Result<Self, Self::Error> {
        AssociationStateProto::decode(stored_state.state.as_slice())?.try_into()
    }
}

impl StoredAssociationState {
    pub fn write_to_cache(
        conn: &DbConnection,
        inbox_id: String,
        sequence_id: i64,
        state: AssociationState,
    ) -> Result<(), StorageError> {
        let state_proto: AssociationStateProto = state.into();
        let result = StoredAssociationState {
            inbox_id: inbox_id.clone(),
            sequence_id,
            state: state_proto.encode_to_vec(),
        }
        .store_or_ignore(conn);

        if result.is_ok() {
            tracing::debug!(
                "Wrote association state to cache: {} {}",
                inbox_id,
                sequence_id
            );
        }

        result
    }

    pub fn read_from_cache(
        conn: &DbConnection,
        inbox_id: impl AsRef<str>,
        sequence_id: i64,
    ) -> Result<Option<AssociationState>, StorageError> {
        let inbox_id = inbox_id.as_ref();
        let stored_state: Option<StoredAssociationState> =
            conn.fetch(&(inbox_id.to_string(), sequence_id))?;

        let result = stored_state
            .map(|stored_state| stored_state.try_into())
            .transpose()?
            .inspect(|_| {
                tracing::debug!(
                    "Loaded association state from cache: {} {}",
                    inbox_id,
                    sequence_id
                )
            });

        Ok(result)
    }

    pub fn batch_read_from_cache(
        conn: &DbConnection,
        identifiers: Vec<(InboxId, i64)>,
    ) -> Result<Vec<AssociationState>, StorageError> {
        if identifiers.is_empty() {
            return Ok(vec![]);
        }

        let (inbox_ids, sequence_ids): (Vec<InboxId>, Vec<i64>) = identifiers.into_iter().unzip();

        let query = dsl::association_state
            .select((dsl::inbox_id, dsl::sequence_id, dsl::state))
            .filter(
                dsl::inbox_id
                    .eq_any(inbox_ids)
                    .and(dsl::sequence_id.eq_any(sequence_ids)),
            );

        let association_states =
            conn.raw_query_read(|query_conn| query.load::<StoredAssociationState>(query_conn))?;

        association_states
            .into_iter()
            .map(|stored_association_state| stored_association_state.try_into())
            .collect::<Result<Vec<AssociationState>, ConversionError>>()
            .map_err(Into::into)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use xmtp_id::associations::Identifier;

    use crate::storage::encrypted_store::tests::with_connection;

    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_batch_read() {
        with_connection(|conn| {
            let association_state = AssociationState::new(
                Identifier::eth("0x1234567890abcdef1234567890abcdef12345678").unwrap(),
                0,
                None,
            )
            .unwrap();
            let inbox_id = association_state.inbox_id().to_string();
            StoredAssociationState::write_to_cache(
                conn,
                inbox_id.to_string(),
                1,
                association_state,
            )
            .unwrap();

            let association_state_2 = AssociationState::new(
                Identifier::eth("0x4567890abcdef1234567890abcdef12345678123").unwrap(),
                2,
                None,
            )
            .unwrap();
            let inbox_id_2 = association_state_2.inbox_id().to_string();
            StoredAssociationState::write_to_cache(
                conn,
                association_state_2.inbox_id().to_string(),
                2,
                association_state_2,
            )
            .unwrap();

            let first_association_state = StoredAssociationState::batch_read_from_cache(
                conn,
                vec![(inbox_id.to_string(), 1)],
            )
            .unwrap();
            assert_eq!(first_association_state.len(), 1);
            assert_eq!(&first_association_state[0].inbox_id(), &inbox_id);

            let both_association_states = StoredAssociationState::batch_read_from_cache(
                conn,
                vec![(inbox_id.to_string(), 1), (inbox_id_2.to_string(), 2)],
            )
            .unwrap();

            assert_eq!(both_association_states.len(), 2);

            let no_results = StoredAssociationState::batch_read_from_cache(
                conn,
                // Mismatched inbox_id and sequence_id
                vec![(inbox_id.to_string(), 2)],
            )
            .unwrap();
            assert_eq!(no_results.len(), 0);
        })
        .await
    }
}
