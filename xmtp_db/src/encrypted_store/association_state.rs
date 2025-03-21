use diesel::prelude::*;

use super::{
    DbConnection,
    schema::association_state::{self, dsl},
};
use crate::{Fetch, StorageError, StoreOrIgnore, impl_fetch, impl_store_or_ignore};

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

impl StoredAssociationState {
    pub fn write_to_cache(
        conn: &DbConnection,
        inbox_id: String,
        sequence_id: i64,
        state: Vec<u8>,
    ) -> Result<(), StorageError> {
        let result = StoredAssociationState {
            inbox_id: inbox_id.clone(),
            sequence_id,
            state,
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

    pub fn read_from_cache<T>(
        conn: &DbConnection,
        inbox_id: impl AsRef<str>,
        sequence_id: i64,
    ) -> Result<Option<T>, StorageError>
    where
        T: TryFrom<StoredAssociationState>,
        StorageError: From<<T as TryFrom<StoredAssociationState>>::Error>,
    {
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

    pub fn batch_read_from_cache<T>(
        conn: &DbConnection,
        identifiers: Vec<(String, i64)>,
    ) -> Result<Vec<T>, StorageError>
    where
        T: TryFrom<StoredAssociationState>,
        StorageError: From<<T as TryFrom<StoredAssociationState>>::Error>,
    {
        if identifiers.is_empty() {
            return Ok(vec![]);
        }

        let (inbox_ids, sequence_ids): (Vec<String>, Vec<i64>) = identifiers.into_iter().unzip();

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
            .collect::<Result<Vec<T>, _>>()
            .map_err(Into::into)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;
    use crate::test_utils::with_connection;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    pub struct MockState {
        sequence_id: u32,
        inbox_id: String,
    }
    impl From<StoredAssociationState> for MockState {
        fn from(v: StoredAssociationState) -> MockState {
            crate::db_deserialize(&v.state).unwrap()
        }
    }

    #[xmtp_common::test]
    async fn test_batch_read() {
        with_connection(|conn| {
            let mock = MockState {
                sequence_id: 1,
                inbox_id: "test_id1".into(),
            };

            StoredAssociationState::write_to_cache(
                conn,
                mock.inbox_id.clone(),
                1,
                crate::db_serialize(&mock).unwrap(),
            )
            .unwrap();

            let mock_2 = MockState {
                sequence_id: 2,
                inbox_id: "test_id2".to_string(),
            };

            StoredAssociationState::write_to_cache(
                conn,
                mock_2.inbox_id.clone(),
                2,
                crate::db_serialize(&mock_2).unwrap(),
            )
            .unwrap();

            let first_association_state: Vec<MockState> =
                StoredAssociationState::batch_read_from_cache(
                    conn,
                    vec![(mock.inbox_id.to_string(), 1)],
                )
                .unwrap();
            assert_eq!(first_association_state.len(), 1);
            assert_eq!(&first_association_state[0].inbox_id, &mock.inbox_id);

            let both_association_states: Vec<MockState> =
                StoredAssociationState::batch_read_from_cache(
                    conn,
                    vec![(mock.inbox_id.clone(), 1), (mock_2.inbox_id.clone(), 2)],
                )
                .unwrap();

            assert_eq!(both_association_states.len(), 2);

            let no_results: Vec<MockState> = StoredAssociationState::batch_read_from_cache(
                conn,
                // Mismatched inbox_id and sequence_id
                vec![(mock.inbox_id.clone(), 2)],
            )
            .unwrap();
            assert_eq!(no_results.len(), 0);
        })
        .await
    }
}
