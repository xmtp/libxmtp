use diesel::prelude::*;

use super::{
    ConnectionExt, DbConnection,
    schema::association_state::{self, dsl},
};
use crate::{Fetch, StorageError, StoreOrIgnore, impl_fetch, impl_store_or_ignore};
use prost::Message;
use xmtp_proto::xmtp::identity::associations::AssociationState as AssociationStateProto;

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

// TODO: We can make a generic trait/object on DB for anything that decodes into prost::Message
// and then have a re-usable cache object instead of re-implementing it on every db type.
impl StoredAssociationState {
    pub fn write_to_cache<C>(
        conn: &DbConnection<C>,
        inbox_id: String,
        sequence_id: i64,
        state: AssociationStateProto,
    ) -> Result<(), StorageError>
    where
        C: ConnectionExt,
    {
        let result = StoredAssociationState {
            inbox_id: inbox_id.clone(),
            sequence_id,
            state: state.encode_to_vec(),
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

    pub fn read_from_cache<T, C: ConnectionExt>(
        conn: &DbConnection<C>,
        inbox_id: impl AsRef<str>,
        sequence_id: i64,
    ) -> Result<Option<T>, StorageError>
    where
        T: TryFrom<AssociationStateProto>,
        StorageError: From<<T as TryFrom<AssociationStateProto>>::Error>,
    {
        let inbox_id = inbox_id.as_ref();
        let stored_state: Option<StoredAssociationState> =
            conn.fetch(&(inbox_id.to_string(), sequence_id))?;

        let result = stored_state
            .map(|stored_state| stored_state.state)
            .inspect(|_| {
                tracing::debug!(
                    "Loaded association state from cache: {} {}",
                    inbox_id,
                    sequence_id
                )
            });
        let decoded = result
            .map(|r| AssociationStateProto::decode(r.as_slice()))
            .transpose()?;
        Ok(decoded.map(|a| a.try_into()).transpose()?)
    }

    pub fn batch_read_from_cache<T, C: ConnectionExt>(
        conn: &DbConnection<C>,
        identifiers: Vec<(String, i64)>,
    ) -> Result<Vec<T>, StorageError>
    where
        T: TryFrom<AssociationStateProto>,
        StorageError: From<<T as TryFrom<AssociationStateProto>>::Error>,
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
            .map(|stored_association_state| {
                AssociationStateProto::decode(stored_association_state.state.as_slice())?
                    .try_into()
                    .map_err(StorageError::from)
            })
            .collect::<Result<Vec<_>, _>>()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;
    use crate::test_utils::with_connection;
    use serde::{Deserialize, Serialize};
    use xmtp_proto::xmtp::identity::associations::AssociationState as AssociationStateProto;

    #[derive(Serialize, Deserialize)]
    pub struct MockState {
        inbox_id: String,
    }
    impl From<StoredAssociationState> for MockState {
        fn from(v: StoredAssociationState) -> MockState {
            crate::db_deserialize(&v.state).unwrap()
        }
    }
    impl From<AssociationStateProto> for MockState {
        fn from(v: AssociationStateProto) -> Self {
            MockState {
                inbox_id: v.inbox_id,
            }
        }
    }

    #[xmtp_common::test]
    async fn test_batch_read() {
        with_connection(|conn| {
            let mock = AssociationStateProto {
                inbox_id: "test_id1".into(),
                members: vec![],
                ..Default::default()
            };

            StoredAssociationState::write_to_cache(conn, mock.inbox_id.clone(), 1, mock.clone())
                .unwrap();
            let mock_2 = AssociationStateProto {
                inbox_id: "test_id2".into(),
                members: vec![],
                ..Default::default()
            };

            StoredAssociationState::write_to_cache(
                conn,
                mock_2.inbox_id.clone(),
                2,
                mock_2.clone(),
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
