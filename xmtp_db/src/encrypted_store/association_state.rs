use diesel::prelude::*;

use super::schema::association_state::{self, dsl};
use crate::ConnectionExt;
use crate::DbConnection;
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

pub trait QueryAssociationStateCache {
    fn write_to_cache(
        &self,
        inbox_id: String,
        sequence_id: i64,
        state: AssociationStateProto,
    ) -> Result<(), StorageError>;

    fn read_from_cache<A: AsRef<str>>(
        &self,
        inbox_id: A,
        sequence_id: i64,
    ) -> Result<Option<AssociationStateProto>, StorageError>;

    fn batch_read_from_cache(
        &self,
        identifiers: Vec<(String, i64)>,
    ) -> Result<Vec<AssociationStateProto>, StorageError>;
}

impl<R> QueryAssociationStateCache for &R
where
    R: QueryAssociationStateCache,
{
    fn write_to_cache(
        &self,
        inbox_id: String,
        sequence_id: i64,
        state: AssociationStateProto,
    ) -> Result<(), StorageError> {
        (**self).write_to_cache(inbox_id, sequence_id, state)
    }

    fn read_from_cache<A: AsRef<str>>(
        &self,
        inbox_id: A,
        sequence_id: i64,
    ) -> Result<Option<AssociationStateProto>, StorageError> {
        (**self).read_from_cache(inbox_id, sequence_id)
    }

    fn batch_read_from_cache(
        &self,
        identifiers: Vec<(String, i64)>,
    ) -> Result<Vec<AssociationStateProto>, StorageError> {
        (**self).batch_read_from_cache(identifiers)
    }
}

impl<C: ConnectionExt> QueryAssociationStateCache for DbConnection<C> {
    fn write_to_cache(
        &self,
        inbox_id: String,
        sequence_id: i64,
        state: AssociationStateProto,
    ) -> Result<(), StorageError> {
        let result = StoredAssociationState {
            inbox_id: inbox_id.clone(),
            sequence_id,
            state: state.encode_to_vec(),
        }
        .store_or_ignore(self);

        if result.is_ok() {
            tracing::debug!(
                "Wrote association state to cache: {} {}",
                inbox_id,
                sequence_id
            );
        }

        result
    }

    fn read_from_cache<A: AsRef<str>>(
        &self,
        inbox_id: A,
        sequence_id: i64,
    ) -> Result<Option<AssociationStateProto>, StorageError> {
        let inbox_id = inbox_id.as_ref();
        let stored_state: Option<StoredAssociationState> =
            self.fetch(&(inbox_id.to_string(), sequence_id))?;

        let result = stored_state
            .map(|stored_state| stored_state.state)
            .inspect(|_| {
                tracing::debug!(
                    "Loaded association state from cache: {} {}",
                    inbox_id,
                    sequence_id
                )
            });
        Ok(result
            .map(|r| AssociationStateProto::decode(r.as_slice()))
            .transpose()?)
    }

    fn batch_read_from_cache(
        &self,
        identifiers: Vec<(String, i64)>,
    ) -> Result<Vec<AssociationStateProto>, StorageError> {
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
            self.raw_query_read(|query_conn| query.load::<StoredAssociationState>(query_conn))?;

        association_states
            .into_iter()
            .map(|stored_association_state| {
                Ok(AssociationStateProto::decode(
                    stored_association_state.state.as_slice(),
                )?)
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
    fn test_batch_read() {
        with_connection(|conn| {
            let mock = AssociationStateProto {
                inbox_id: "test_id1".into(),
                members: vec![],
                ..Default::default()
            };
            conn.write_to_cache(mock.inbox_id.clone(), 1, mock.clone())
                .unwrap();
            let mock_2 = AssociationStateProto {
                inbox_id: "test_id2".into(),
                members: vec![],
                ..Default::default()
            };

            conn.write_to_cache(mock_2.inbox_id.clone(), 2, mock_2.clone())
                .unwrap();

            let first_association_state: Vec<MockState> = conn
                .batch_read_from_cache(vec![(mock.inbox_id.to_string(), 1)])
                .unwrap()
                .into_iter()
                .map(Into::into)
                .collect();
            assert_eq!(first_association_state.len(), 1);
            assert_eq!(&first_association_state[0].inbox_id, &mock.inbox_id);

            let both_association_states: Vec<MockState> = conn
                .batch_read_from_cache(vec![
                    (mock.inbox_id.clone(), 1),
                    (mock_2.inbox_id.clone(), 2),
                ])
                .unwrap()
                .into_iter()
                .map(Into::into)
                .collect();

            assert_eq!(both_association_states.len(), 2);

            let no_results = conn
                .batch_read_from_cache(vec![(mock.inbox_id.clone(), 2)])
                .unwrap()
                .into_iter()
                .map(Into::into)
                .collect::<Vec<MockState>>();
            assert_eq!(no_results.len(), 0);
        })
    }
}
