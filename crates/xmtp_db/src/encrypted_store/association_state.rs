#[cfg(feature = "sync")]
use diesel::prelude::*;

#[cfg(feature = "sync")]
use super::schema::association_state::{self, dsl};
#[cfg(feature = "sync")]
use crate::ConnectionExt;
#[cfg(feature = "sync")]
use crate::DbConnection;
use crate::StorageError;
#[cfg(feature = "sync")]
use crate::{Fetch, StoreOrIgnore, impl_fetch, impl_store_or_ignore};
use prost::Message;
use xmtp_proto::xmtp::identity::associations::AssociationState as AssociationStateProto;

/// StoredIdentityUpdate holds a serialized IdentityUpdate record
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "sync", derive(Insertable, Identifiable, Queryable))]
#[cfg_attr(feature = "sync", diesel(table_name = association_state))]
#[cfg_attr(feature = "sync", diesel(primary_key(inbox_id, sequence_id)))]
pub struct StoredAssociationState {
    pub inbox_id: String,
    pub sequence_id: i64,
    pub state: Vec<u8>,
}
#[cfg(feature = "sync")]
impl_fetch!(StoredAssociationState, association_state, (String, i64));
#[cfg(feature = "sync")]
impl_store_or_ignore!(StoredAssociationState, association_state);

pub trait QueryAssociationStateCache {
    fn write_to_cache(
        &self,
        inbox_id: String,
        sequence_id: i64,
        state: AssociationStateProto,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + xmtp_common::MaybeSend;

    fn read_from_cache(
        &self,
        inbox_id: &str,
        sequence_id: i64,
    ) -> impl std::future::Future<Output = Result<Option<AssociationStateProto>, StorageError>>
    + xmtp_common::MaybeSend;

    fn batch_read_from_cache(
        &self,
        identifiers: Vec<(String, i64)>,
    ) -> impl std::future::Future<Output = Result<Vec<AssociationStateProto>, StorageError>>
    + xmtp_common::MaybeSend;
}

impl<R> QueryAssociationStateCache for &R
where
    R: QueryAssociationStateCache + xmtp_common::MaybeSync,
{
    async fn write_to_cache(
        &self,
        inbox_id: String,
        sequence_id: i64,
        state: AssociationStateProto,
    ) -> Result<(), StorageError> {
        (**self).write_to_cache(inbox_id, sequence_id, state).await
    }

    async fn read_from_cache(
        &self,
        inbox_id: &str,
        sequence_id: i64,
    ) -> Result<Option<AssociationStateProto>, StorageError> {
        (**self).read_from_cache(inbox_id, sequence_id).await
    }

    async fn batch_read_from_cache(
        &self,
        identifiers: Vec<(String, i64)>,
    ) -> Result<Vec<AssociationStateProto>, StorageError> {
        (**self).batch_read_from_cache(identifiers).await
    }
}

#[cfg(feature = "sync")]
impl<C: ConnectionExt> QueryAssociationStateCache for DbConnection<C> {
    async fn write_to_cache(
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
        .store_or_ignore(self)
        .await;

        if result.is_ok() {
            tracing::debug!(
                "Wrote association state to cache: {} {}",
                inbox_id,
                sequence_id
            );
        }

        result
    }

    async fn read_from_cache(
        &self,
        inbox_id: &str,
        sequence_id: i64,
    ) -> Result<Option<AssociationStateProto>, StorageError> {
        // The AFIT `fetch` returns an opaque `impl Future`, so the `Model` type
        // parameter can no longer be inferred through it from the binding alone;
        // name it explicitly via UFCS.
        let stored_state: Option<StoredAssociationState> =
            <DbConnection<C> as Fetch<StoredAssociationState>>::fetch(
                self,
                &(inbox_id.to_string(), sequence_id),
            )
            .await?;

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

    async fn batch_read_from_cache(
        &self,
        identifiers: Vec<(String, i64)>,
    ) -> Result<Vec<AssociationStateProto>, StorageError> {
        if identifiers.is_empty() {
            return Ok(vec![]);
        }

        let mut query = dsl::association_state
            .select((dsl::inbox_id, dsl::sequence_id, dsl::state))
            .into_boxed();

        for (inbox_id, sequence_id) in &identifiers {
            let predicate = dsl::inbox_id
                .eq(inbox_id.clone())
                .and(dsl::sequence_id.eq(*sequence_id));
            query = query.or_filter(predicate);
        }

        let association_states =
            self.raw_query(|query_conn| query.load::<StoredAssociationState>(query_conn))?;

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

/// sqlx backend -- Postgres only. See the note on `QueryGroupVersion`'s impl for
/// why this is gated `not(feature = "sync")`.
#[cfg(all(feature = "async", not(feature = "sync"), not(target_arch = "wasm32")))]
impl QueryAssociationStateCache for crate::pg::PgDb {
    /// `store_or_ignore` on the sync track is SQLite's `INSERT OR IGNORE`;
    /// `ON CONFLICT DO NOTHING` is the Postgres equivalent. An existing row is
    /// left as-is rather than overwritten — association state for a given
    /// (inbox_id, sequence_id) is immutable, so a second write is redundant.
    async fn write_to_cache(
        &self,
        inbox_id: String,
        sequence_id: i64,
        state: AssociationStateProto,
    ) -> Result<(), StorageError> {
        let mut c = self.conn().await?;
        sqlx::query(
            "INSERT INTO association_state (inbox_id, sequence_id, state) \
             VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(&inbox_id)
        .bind(sequence_id)
        .bind(state.encode_to_vec())
        .execute(&mut *c)
        .await
        .map_err(crate::ConnectionError::from)?;

        tracing::debug!(
            "Wrote association state to cache: {} {}",
            inbox_id,
            sequence_id
        );
        Ok(())
    }

    async fn read_from_cache(
        &self,
        inbox_id: &str,
        sequence_id: i64,
    ) -> Result<Option<AssociationStateProto>, StorageError> {
        use sqlx::Row;

        let mut c = self.conn().await?;
        let row = sqlx::query(
            "SELECT state FROM association_state WHERE inbox_id = $1 AND sequence_id = $2",
        )
        .bind(inbox_id)
        .bind(sequence_id)
        .fetch_optional(&mut *c)
        .await
        .map_err(crate::ConnectionError::from)?;

        let Some(row) = row else {
            return Ok(None);
        };
        tracing::debug!(
            "Loaded association state from cache: {} {}",
            inbox_id,
            sequence_id
        );
        let state: Vec<u8> = row.try_get(0).map_err(crate::ConnectionError::from)?;
        Ok(Some(AssociationStateProto::decode(state.as_slice())?))
    }

    async fn batch_read_from_cache(
        &self,
        identifiers: Vec<(String, i64)>,
    ) -> Result<Vec<AssociationStateProto>, StorageError> {
        use sqlx::Row;
        // Same guard as the diesel impl: with no identifiers the OR-chain there
        // degenerates to an unfiltered query that would load the whole table.
        if identifiers.is_empty() {
            return Ok(vec![]);
        }

        // The pairs go over as two parallel arrays zipped back together by
        // `UNNEST`, so this stays one prepared statement no matter how many
        // identifiers are asked for.
        let (inbox_ids, sequence_ids): (Vec<String>, Vec<i64>) = identifiers.into_iter().unzip();

        let mut c = self.conn().await?;
        let rows = sqlx::query(
            "SELECT state FROM association_state \
             WHERE (inbox_id, sequence_id) IN (SELECT * FROM UNNEST($1::text[], $2::int8[]))",
        )
        .bind(&inbox_ids)
        .bind(&sequence_ids)
        .fetch_all(&mut *c)
        .await
        .map_err(crate::ConnectionError::from)?;

        rows.into_iter()
            .map(|row| {
                let state: Vec<u8> = row.try_get(0).map_err(crate::ConnectionError::from)?;
                Ok(AssociationStateProto::decode(state.as_slice())?)
            })
            .collect()
    }
}

#[cfg(test)]
pub(crate) mod tests {
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
        with_connection(async |conn| {
            let mock = AssociationStateProto {
                inbox_id: "test_id1".into(),
                members: vec![],
                ..Default::default()
            };
            conn.write_to_cache(mock.inbox_id.clone(), 1, mock.clone())
                .await
                .unwrap();
            let mock_2 = AssociationStateProto {
                inbox_id: "test_id2".into(),
                members: vec![],
                ..Default::default()
            };

            conn.write_to_cache(mock_2.inbox_id.clone(), 2, mock_2.clone())
                .await
                .unwrap();

            let first_association_state: Vec<MockState> = conn
                .batch_read_from_cache(vec![(mock.inbox_id.to_string(), 1)])
                .await
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
                .await
                .unwrap()
                .into_iter()
                .map(Into::into)
                .collect();

            assert_eq!(both_association_states.len(), 2);

            let no_results = conn
                .batch_read_from_cache(vec![(mock.inbox_id.clone(), 2)])
                .await
                .unwrap()
                .into_iter()
                .map(Into::into)
                .collect::<Vec<MockState>>();
            assert_eq!(no_results.len(), 0);
        })
        .await
    }
}
