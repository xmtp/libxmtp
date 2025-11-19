use std::collections::HashMap;

use derive_builder::Builder;
use diesel::{
    backend::Backend,
    connection::DefaultLoadingMode,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Integer,
};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use xmtp_common::fmt;
use xmtp_proto::types::Cursor;

use super::{
    ConnectionExt, Sqlite,
    db_connection::DbConnection,
    group,
    schema::group_intents::{self, dsl},
};
use crate::{
    Delete, NotFound, StorageError,
    group::StoredGroup,
    group_message::{GroupMessageKind, QueryGroupMessage},
    impl_fetch, impl_store,
};
pub type ID = i32;

mod error;
mod types;
pub use error::*;
pub use types::*;

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, AsExpression, FromSqlRow, Serialize, Deserialize)]
#[diesel(sql_type = Integer)]
pub enum IntentKind {
    SendMessage = 1,
    KeyUpdate = 2,
    MetadataUpdate = 3,
    UpdateGroupMembership = 4,
    UpdateAdminList = 5,
    UpdatePermission = 6,
    ReaddInstallations = 7,
}

impl std::fmt::Display for IntentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let description = match self {
            IntentKind::SendMessage => "SendMessage",
            IntentKind::KeyUpdate => "KeyUpdate",
            IntentKind::MetadataUpdate => "MetadataUpdate",
            IntentKind::UpdateGroupMembership => "UpdateGroupMembership",
            IntentKind::UpdateAdminList => "UpdateAdminList",
            IntentKind::UpdatePermission => "UpdatePermission",
            IntentKind::ReaddInstallations => "ReaddInstallations",
        };
        write!(f, "{}", description)
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, AsExpression, FromSqlRow)]
#[diesel(sql_type = Integer)]
pub enum IntentState {
    ToPublish = 1,
    Published = 2,
    Committed = 3,
    Error = 4,
    Processed = 5,
}

#[derive(Queryable, Identifiable, Selectable, Associations, PartialEq, Clone)]
#[diesel(belongs_to(StoredGroup, foreign_key = group_id))]
#[diesel(table_name = group_intents)]
#[diesel(primary_key(id))]
pub struct StoredGroupIntent {
    pub id: ID,
    pub kind: IntentKind,
    pub group_id: group::ID,
    pub data: Vec<u8>,
    pub state: IntentState,
    pub payload_hash: Option<Vec<u8>>,
    pub post_commit_data: Option<Vec<u8>>,
    pub publish_attempts: i32,
    pub staged_commit: Option<Vec<u8>>,
    pub published_in_epoch: Option<i64>,
    pub should_push: bool,
    pub sequence_id: Option<i64>,
    pub originator_id: Option<i64>,
}

impl std::fmt::Debug for StoredGroupIntent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StoredGroupIntent {{ ")?;
        write!(f, "id: {}, ", self.id)?;
        write!(f, "kind: {}, ", self.kind)?;
        write!(
            f,
            "group_id: {}, ",
            fmt::truncate_hex(hex::encode(&self.group_id))
        )?;
        write!(f, "data: {}, ", fmt::truncate_hex(hex::encode(&self.data)))?;
        write!(f, "state: {:?}, ", self.state)?;
        write!(
            f,
            "payload_hash: {:?}, ",
            self.payload_hash
                .as_ref()
                .map(|h| fmt::truncate_hex(hex::encode(h)))
        )?;
        write!(
            f,
            "post_commit_data: {:?}, ",
            self.post_commit_data
                .as_ref()
                .map(|d| fmt::truncate_hex(hex::encode(d)))
        )?;
        write!(f, "publish_attempts: {:?}, ", self.publish_attempts)?;
        write!(
            f,
            "staged_commit: {:?}, ",
            self.staged_commit
                .as_ref()
                .map(|c| fmt::truncate_hex(hex::encode(c)))
        )?;
        write!(f, "published_in_epoch: {:?} ", self.published_in_epoch)?;
        write!(f, " }}")?;
        Ok(())
    }
}

impl_fetch!(StoredGroupIntent, group_intents, ID);

impl<C: ConnectionExt> Delete<StoredGroupIntent> for DbConnection<C> {
    type Key = ID;
    fn delete(&self, key: ID) -> Result<usize, StorageError> {
        Ok(self.raw_query_write(|raw_conn| {
            diesel::delete(dsl::group_intents.find(key)).execute(raw_conn)
        })?)
    }
}

/// NewGroupIntent is the data needed to create a new group intent.
/// Do not use this struct directly outside of the storage module.
/// Use the `queue_intent` method on `MlsGroup` instead.
#[derive(Insertable, Debug, PartialEq, Clone, Builder)]
#[diesel(table_name = group_intents)]
#[builder(setter(into), build_fn(error = "StorageError"))]
pub struct NewGroupIntent {
    pub kind: IntentKind,
    pub group_id: Vec<u8>,
    pub data: Vec<u8>,
    pub should_push: bool,
    #[builder(default = "IntentState::ToPublish")]
    pub state: IntentState,
}

impl_store!(NewGroupIntent, group_intents);

impl NewGroupIntent {
    pub fn builder() -> NewGroupIntentBuilder {
        NewGroupIntentBuilder::default()
    }

    pub fn new(kind: IntentKind, group_id: Vec<u8>, data: Vec<u8>, should_push: bool) -> Self {
        Self {
            kind,
            group_id,
            data,
            state: IntentState::ToPublish,
            should_push,
        }
    }
}

pub trait QueryGroupIntent {
    fn insert_group_intent(
        &self,
        to_save: NewGroupIntent,
    ) -> Result<StoredGroupIntent, crate::ConnectionError>;

    // Query for group_intents by group_id, optionally filtering by state and kind
    fn find_group_intents(
        &self,
        group_id: Vec<u8>,
        allowed_states: Option<Vec<IntentState>>,
        allowed_kinds: Option<Vec<IntentKind>>,
    ) -> Result<Vec<StoredGroupIntent>, crate::ConnectionError>;

    // Set the intent with the given ID to `Published` and set the payload hash. Optionally add
    // `post_commit_data`
    fn set_group_intent_published(
        &self,
        intent_id: ID,
        payload_hash: &[u8],
        post_commit_data: Option<Vec<u8>>,
        staged_commit: Option<Vec<u8>>,
        published_in_epoch: i64,
    ) -> Result<(), StorageError>;

    // Set the intent with the given ID to `Committed`
    fn set_group_intent_committed(&self, intent_id: ID, cursor: Cursor)
    -> Result<(), StorageError>;

    // Set the intent with the given ID to `Committed`
    fn set_group_intent_processed(&self, intent_id: ID) -> Result<(), StorageError>;

    // Set the intent with the given ID to `ToPublish`. Wipe any values for `payload_hash` and
    // `post_commit_data`
    fn set_group_intent_to_publish(&self, intent_id: ID) -> Result<(), StorageError>;

    /// Set the intent with the given ID to `Error`
    fn set_group_intent_error(&self, intent_id: ID) -> Result<(), StorageError>;

    // Simple lookup of intents by payload hash, meant to be used when processing messages off the
    // network
    fn find_group_intent_by_payload_hash(
        &self,
        payload_hash: &[u8],
    ) -> Result<Option<StoredGroupIntent>, StorageError>;

    /// find the messages these intents depend on, returning results in the same order as the input hashes
    fn find_dependant_commits<P: AsRef<[u8]>>(
        &self,
        payload_hashes: &[P],
    ) -> Result<HashMap<PayloadHash, IntentDependency>, StorageError>;
    fn increment_intent_publish_attempt_count(&self, intent_id: ID) -> Result<(), StorageError>;

    fn set_group_intent_error_and_fail_msg(
        &self,
        intent: &StoredGroupIntent,
        msg_id: Option<Vec<u8>>,
    ) -> Result<(), StorageError>;
}

impl<T> QueryGroupIntent for &T
where
    T: QueryGroupIntent,
{
    fn insert_group_intent(
        &self,
        to_save: NewGroupIntent,
    ) -> Result<StoredGroupIntent, crate::ConnectionError> {
        (**self).insert_group_intent(to_save)
    }

    fn find_group_intents(
        &self,
        group_id: Vec<u8>,
        allowed_states: Option<Vec<IntentState>>,
        allowed_kinds: Option<Vec<IntentKind>>,
    ) -> Result<Vec<StoredGroupIntent>, crate::ConnectionError> {
        (**self).find_group_intents(group_id, allowed_states, allowed_kinds)
    }

    fn set_group_intent_published(
        &self,
        intent_id: ID,
        payload_hash: &[u8],
        post_commit_data: Option<Vec<u8>>,
        staged_commit: Option<Vec<u8>>,
        published_in_epoch: i64,
    ) -> Result<(), StorageError> {
        (**self).set_group_intent_published(
            intent_id,
            payload_hash,
            post_commit_data,
            staged_commit,
            published_in_epoch,
        )
    }

    fn set_group_intent_committed(
        &self,
        intent_id: ID,
        cursor: Cursor,
    ) -> Result<(), StorageError> {
        (**self).set_group_intent_committed(intent_id, cursor)
    }

    fn set_group_intent_processed(&self, intent_id: ID) -> Result<(), StorageError> {
        (**self).set_group_intent_processed(intent_id)
    }

    fn set_group_intent_to_publish(&self, intent_id: ID) -> Result<(), StorageError> {
        (**self).set_group_intent_to_publish(intent_id)
    }

    fn set_group_intent_error(&self, intent_id: ID) -> Result<(), StorageError> {
        (**self).set_group_intent_error(intent_id)
    }

    fn find_group_intent_by_payload_hash(
        &self,
        payload_hash: &[u8],
    ) -> Result<Option<StoredGroupIntent>, StorageError> {
        (**self).find_group_intent_by_payload_hash(payload_hash)
    }

    fn find_dependant_commits<P: AsRef<[u8]>>(
        &self,
        payload_hashes: &[P],
    ) -> Result<HashMap<PayloadHash, IntentDependency>, StorageError> {
        (**self).find_dependant_commits(payload_hashes)
    }

    fn increment_intent_publish_attempt_count(&self, intent_id: ID) -> Result<(), StorageError> {
        (**self).increment_intent_publish_attempt_count(intent_id)
    }

    fn set_group_intent_error_and_fail_msg(
        &self,
        intent: &StoredGroupIntent,
        msg_id: Option<Vec<u8>>,
    ) -> Result<(), StorageError> {
        (**self).set_group_intent_error_and_fail_msg(intent, msg_id)
    }
}

impl<C: ConnectionExt> QueryGroupIntent for DbConnection<C> {
    #[tracing::instrument(level = "trace", skip(self))]
    fn insert_group_intent(
        &self,
        to_save: NewGroupIntent,
    ) -> Result<StoredGroupIntent, crate::ConnectionError> {
        self.raw_query_write(|conn| {
            diesel::insert_into(dsl::group_intents)
                .values(to_save)
                .get_result(conn)
        })
    }

    // Query for group_intents by group_id, optionally filtering by state and kind
    #[tracing::instrument(level = "trace", skip(self))]
    fn find_group_intents(
        &self,
        group_id: Vec<u8>,
        allowed_states: Option<Vec<IntentState>>,
        allowed_kinds: Option<Vec<IntentKind>>,
    ) -> Result<Vec<StoredGroupIntent>, crate::ConnectionError> {
        let mut query = dsl::group_intents
            .into_boxed()
            .filter(dsl::group_id.eq(group_id));

        if let Some(allowed_states) = allowed_states {
            query = query.filter(dsl::state.eq_any(allowed_states));
        }

        if let Some(allowed_kinds) = allowed_kinds {
            query = query.filter(dsl::kind.eq_any(allowed_kinds));
        }

        query = query.order(dsl::id.asc());

        self.raw_query_read(|conn| query.load::<StoredGroupIntent>(conn))
    }

    // Set the intent with the given ID to `Published` and set the payload hash. Optionally add
    // `post_commit_data`
    fn set_group_intent_published(
        &self,
        intent_id: ID,
        payload_hash: &[u8],
        post_commit_data: Option<Vec<u8>>,
        staged_commit: Option<Vec<u8>>,
        published_in_epoch: i64,
    ) -> Result<(), StorageError> {
        let rows_changed = self.raw_query_write(|conn| {
            diesel::update(dsl::group_intents)
                .filter(dsl::id.eq(intent_id))
                // State machine requires that the only valid state transition to Published is from
                // ToPublish
                .filter(dsl::state.eq(IntentState::ToPublish))
                .set((
                    dsl::state.eq(IntentState::Published),
                    dsl::payload_hash.eq(payload_hash),
                    dsl::post_commit_data.eq(post_commit_data),
                    dsl::staged_commit.eq(staged_commit),
                    dsl::published_in_epoch.eq(published_in_epoch),
                ))
                .execute(conn)
        })?;

        if rows_changed == 0 {
            let already_published = self.raw_query_read(|conn| {
                dsl::group_intents
                    .filter(dsl::id.eq(intent_id))
                    .first::<StoredGroupIntent>(conn)
            });

            if already_published.is_ok() {
                return Ok(());
            } else {
                return Err(NotFound::IntentForToPublish(intent_id).into());
            }
        }
        Ok(())
    }

    // Set the intent with the given ID to `Committed`
    fn set_group_intent_committed(
        &self,
        intent_id: ID,
        cursor: Cursor,
    ) -> Result<(), StorageError> {
        let rows_changed: usize = self.raw_query_write(|conn| {
            diesel::update(dsl::group_intents)
                .filter(dsl::id.eq(intent_id))
                // State machine requires that the only valid state transition to Committed is from
                // Published
                .filter(dsl::state.eq(IntentState::Published))
                .set((
                    dsl::state.eq(IntentState::Committed),
                    dsl::sequence_id.eq(cursor.sequence_id as i64),
                    dsl::originator_id.eq(cursor.originator_id as i64),
                ))
                .execute(conn)
        })?;

        // If nothing matched the query, return an error. Either ID or state was wrong
        if rows_changed == 0 {
            return Err(NotFound::IntentForCommitted(intent_id).into());
        }

        Ok(())
    }

    // Set the intent with the given ID to `Committed`
    fn set_group_intent_processed(&self, intent_id: ID) -> Result<(), StorageError> {
        let rows_changed = self.raw_query_write(|conn| {
            diesel::update(dsl::group_intents)
                .filter(dsl::id.eq(intent_id))
                .set(dsl::state.eq(IntentState::Processed))
                .execute(conn)
        })?;

        // If nothing matched the query, return an error. Either ID or state was wrong
        if rows_changed == 0 {
            return Err(NotFound::IntentById(intent_id).into());
        }

        Ok(())
    }

    // Set the intent with the given ID to `ToPublish`. Wipe any values for `payload_hash` and
    // `post_commit_data`
    fn set_group_intent_to_publish(&self, intent_id: ID) -> Result<(), StorageError> {
        let rows_changed = self.raw_query_write(|conn| {
            diesel::update(dsl::group_intents)
                .filter(dsl::id.eq(intent_id))
                // State machine requires that the only valid state transition to ToPublish is from
                // Published
                .filter(dsl::state.eq(IntentState::Published))
                .set((
                    dsl::state.eq(IntentState::ToPublish),
                    // When moving to ToPublish, clear the payload hash and post commit data
                    dsl::payload_hash.eq(None::<Vec<u8>>),
                    dsl::post_commit_data.eq(None::<Vec<u8>>),
                    dsl::published_in_epoch.eq(None::<i64>),
                    dsl::staged_commit.eq(None::<Vec<u8>>),
                ))
                .execute(conn)
        })?;

        if rows_changed == 0 {
            return Err(NotFound::IntentForPublish(intent_id).into());
        }
        Ok(())
    }

    /// Set the intent with the given ID to `Error`
    #[tracing::instrument(level = "trace", skip(self))]
    fn set_group_intent_error(&self, intent_id: ID) -> Result<(), StorageError> {
        let rows_changed = self.raw_query_write(|conn| {
            diesel::update(dsl::group_intents)
                .filter(dsl::id.eq(intent_id))
                .set(dsl::state.eq(IntentState::Error))
                .execute(conn)
        })?;

        if rows_changed == 0 {
            return Err(NotFound::IntentById(intent_id).into());
        }

        Ok(())
    }

    // Simple lookup of intents by payload hash, meant to be used when processing messages off the
    // network
    #[tracing::instrument(
        level = "trace",
        skip_all,
        fields(payload_hash = hex::encode(payload_hash))
    )]
    fn find_group_intent_by_payload_hash(
        &self,
        payload_hash: &[u8],
    ) -> Result<Option<StoredGroupIntent>, StorageError> {
        let result = self.raw_query_read(|conn| {
            dsl::group_intents
                .filter(dsl::payload_hash.eq(payload_hash))
                .first::<StoredGroupIntent>(conn)
                .optional()
        })?;

        Ok(result)
    }

    /// try to find dependencies of [`payload_hash`].
    /// a message should always have a dependency.
    fn find_dependant_commits<P: AsRef<[u8]>>(
        &self,
        payload_hashes: &[P],
    ) -> Result<HashMap<PayloadHash, IntentDependency>, StorageError> {
        use super::schema::group_messages;
        let hashes = payload_hashes
            .iter()
            .map(|h| PayloadHashRef::from(h.as_ref()));
        // Query all dependencies in a single database call
        let map: HashMap<PayloadHash, Vec<IntentDependency>> = self.raw_query_read(|conn| {
            dsl::group_intents
                .filter(dsl::payload_hash.eq_any(hashes))
                .filter(dsl::published_in_epoch.is_not_null())
                .inner_join(
                    group_messages::table.on(group_messages::group_id
                        .eq(dsl::group_id)
                        .and(group_messages::kind.eq(GroupMessageKind::MembershipChange))
                        .and(group_messages::published_in_epoch.eq(dsl::published_in_epoch - 1))),
                )
                .select((
                    dsl::payload_hash.assume_not_null(),
                    group_messages::sequence_id,
                    group_messages::originator_id,
                    dsl::group_id,
                ))
                .load_iter::<(Vec<u8>, i64, i64, Vec<u8>), DefaultLoadingMode>(conn)?
                .map_ok(|(hash, sequence_id, originator_id, group_id)| {
                    (
                        PayloadHash::from(hash),
                        IntentDependency {
                            cursor: Cursor {
                                sequence_id: sequence_id as u64,
                                originator_id: originator_id as u32,
                            },
                            group_id: group_id.into(),
                        },
                    )
                })
                .process_results(|iter| iter.into_grouping_map().collect())
        })?;

        // Check for multiple dependencies and build result in input order
        // NOTE: since we halt processing if a single hash has more than one dependency,
        // it is possible that other groups are valid but their hashes will not be sent b/c of the
        // error.
        // in practice, we send one message at a time so this should not pose an issue.
        for (hash, deps) in &map {
            if deps.len() > 1 {
                return Err(GroupIntentError::MoreThanTwoDependencies {
                    payload_hash: hash.clone(),
                    cursors: deps.iter().map(|d| d.cursor).collect(),
                    group_id: deps[0].group_id.clone(),
                }
                .into());
            }
        }

        map.into_iter()
            .map(|(hash, mut d)| {
                // this should be impossible since the sql query wouldnt return anything for
                // an empty payload hash.
                let dep = d
                    .pop()
                    .ok_or_else(|| GroupIntentError::NoDependencyFound {
                        hash: hash.clone().into(),
                    })
                    .map_err(StorageError::from)?;
                Ok((hash, dep))
            })
            .try_collect()
    }

    fn increment_intent_publish_attempt_count(&self, intent_id: ID) -> Result<(), StorageError> {
        self.raw_query_write(|conn| {
            diesel::update(dsl::group_intents)
                .filter(dsl::id.eq(intent_id))
                .set(dsl::publish_attempts.eq(dsl::publish_attempts + 1))
                .execute(conn)
        })?;

        Ok(())
    }

    fn set_group_intent_error_and_fail_msg(
        &self,
        intent: &StoredGroupIntent,
        msg_id: Option<Vec<u8>>,
    ) -> Result<(), StorageError> {
        self.set_group_intent_error(intent.id)?;
        if let Some(id) = msg_id {
            self.set_delivery_status_to_failed(&id)?;
        }
        Ok(())
    }
}

impl ToSql<Integer, Sqlite> for IntentKind
where
    i32: ToSql<Integer, Sqlite>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(IsNull::No)
    }
}

impl FromSql<Integer, Sqlite> for IntentKind
where
    i32: FromSql<Integer, Sqlite>,
{
    fn from_sql(bytes: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        match i32::from_sql(bytes)? {
            1 => Ok(IntentKind::SendMessage),
            2 => Ok(IntentKind::KeyUpdate),
            3 => Ok(IntentKind::MetadataUpdate),
            4 => Ok(IntentKind::UpdateGroupMembership),
            5 => Ok(IntentKind::UpdateAdminList),
            6 => Ok(IntentKind::UpdatePermission),
            7 => Ok(IntentKind::ReaddInstallations),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}

impl ToSql<Integer, Sqlite> for IntentState
where
    i32: ToSql<Integer, Sqlite>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(IsNull::No)
    }
}

impl FromSql<Integer, Sqlite> for IntentState
where
    i32: FromSql<Integer, Sqlite>,
{
    fn from_sql(bytes: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        match i32::from_sql(bytes)? {
            1 => Ok(IntentState::ToPublish),
            2 => Ok(IntentState::Published),
            3 => Ok(IntentState::Committed),
            4 => Ok(IntentState::Error),
            5 => Ok(IntentState::Processed),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;
    use crate::{
        Fetch, Store,
        group::{GroupMembershipState, StoredGroup},
        test_utils::with_connection,
    };
    use proptest::{collection, prelude::*};
    use xmtp_common::rand_vec;

    fn insert_group<C: ConnectionExt>(conn: &DbConnection<C>, group_id: Vec<u8>) {
        StoredGroup::builder()
            .id(group_id)
            .created_at_ns(100)
            .membership_state(GroupMembershipState::Allowed)
            .added_by_inbox_id("placeholder_address")
            .build()
            .unwrap()
            .store(conn)
            .unwrap();
    }

    impl NewGroupIntent {
        // Real group intents must always start as ToPublish. But for tests we allow forcing the
        // state
        pub fn new_test(
            kind: IntentKind,
            group_id: Vec<u8>,
            data: Vec<u8>,
            state: IntentState,
        ) -> Self {
            Self {
                kind,
                group_id,
                data,
                state,
                should_push: false,
            }
        }
    }

    fn find_first_intent<C: ConnectionExt>(
        conn: &DbConnection<C>,
        group_id: group::ID,
    ) -> StoredGroupIntent {
        conn.raw_query_read(|raw_conn| {
            dsl::group_intents
                .filter(dsl::group_id.eq(group_id))
                .first(raw_conn)
        })
        .unwrap()
    }

    #[xmtp_common::test]
    async fn test_store_and_fetch() {
        let group_id = rand_vec::<24>();
        let data = rand_vec::<24>();
        let kind = IntentKind::UpdateGroupMembership;
        let state = IntentState::ToPublish;

        let to_insert = NewGroupIntent::new_test(kind, group_id.clone(), data.clone(), state);

        with_connection(|conn| {
            // Group needs to exist or FK constraint will fail
            insert_group(conn, group_id.clone());

            to_insert.store(conn).unwrap();

            let results = conn
                .find_group_intents(group_id.clone(), Some(vec![IntentState::ToPublish]), None)
                .unwrap();

            assert_eq!(results.len(), 1);
            assert_eq!(results[0].kind, kind);
            assert_eq!(results[0].data, data);
            assert_eq!(results[0].group_id, group_id);

            let id = results[0].id;

            let fetched: StoredGroupIntent = conn.fetch(&id).unwrap().unwrap();

            assert_eq!(fetched.id, id);
        })
        .await
    }

    #[xmtp_common::test]
    async fn test_query() {
        let group_id = rand_vec::<24>();

        let test_intents: Vec<NewGroupIntent> = vec![
            NewGroupIntent::new_test(
                IntentKind::UpdateGroupMembership,
                group_id.clone(),
                rand_vec::<24>(),
                IntentState::ToPublish,
            ),
            NewGroupIntent::new_test(
                IntentKind::KeyUpdate,
                group_id.clone(),
                rand_vec::<24>(),
                IntentState::Published,
            ),
            NewGroupIntent::new_test(
                IntentKind::KeyUpdate,
                group_id.clone(),
                rand_vec::<24>(),
                IntentState::Committed,
            ),
        ];

        with_connection(|conn| {
            // Group needs to exist or FK constraint will fail
            insert_group(conn, group_id.clone());

            for case in test_intents {
                case.store(conn).unwrap();
            }

            // Can query for multiple states
            let mut results = conn
                .find_group_intents(
                    group_id.clone(),
                    Some(vec![IntentState::ToPublish, IntentState::Published]),
                    None,
                )
                .unwrap();

            assert_eq!(results.len(), 2);

            // Can query by kind
            results = conn
                .find_group_intents(group_id.clone(), None, Some(vec![IntentKind::KeyUpdate]))
                .unwrap();
            assert_eq!(results.len(), 2);

            // Can query by kind and state
            results = conn
                .find_group_intents(
                    group_id.clone(),
                    Some(vec![IntentState::Committed]),
                    Some(vec![IntentKind::KeyUpdate]),
                )
                .unwrap();

            assert_eq!(results.len(), 1);

            // Can get no results
            results = conn
                .find_group_intents(
                    group_id.clone(),
                    Some(vec![IntentState::Committed]),
                    Some(vec![IntentKind::SendMessage]),
                )
                .unwrap();

            assert_eq!(results.len(), 0);

            // Can get all intents
            results = conn.find_group_intents(group_id, None, None).unwrap();
            assert_eq!(results.len(), 3);
        })
        .await
    }

    #[xmtp_common::test]
    async fn find_by_payload_hash() {
        let group_id = rand_vec::<24>();

        with_connection(|conn| {
            insert_group(conn, group_id.clone());

            // Store the intent
            NewGroupIntent::new(
                IntentKind::UpdateGroupMembership,
                group_id.clone(),
                rand_vec::<24>(),
                false,
            )
            .store(conn)
            .unwrap();

            // Find the intent with the ID populated
            let intent = find_first_intent(conn, group_id.clone());

            // Set the payload hash
            let payload_hash = rand_vec::<24>();
            let post_commit_data = rand_vec::<24>();
            conn.set_group_intent_published(
                intent.id,
                &payload_hash,
                Some(post_commit_data.clone()),
                None,
                1,
            )
            .unwrap();

            let find_result = conn
                .find_group_intent_by_payload_hash(&payload_hash)
                .unwrap()
                .unwrap();

            assert_eq!(find_result.id, intent.id);
            assert_eq!(find_result.published_in_epoch, Some(1));
        })
        .await
    }

    #[xmtp_common::test]
    async fn test_happy_path_state_transitions() {
        let group_id = rand_vec::<24>();

        with_connection(|conn| {
            insert_group(conn, group_id.clone());

            // Store the intent
            NewGroupIntent::new(
                IntentKind::UpdateGroupMembership,
                group_id.clone(),
                rand_vec::<24>(),
                false,
            )
            .store(conn)
            .unwrap();

            let mut intent = find_first_intent(conn, group_id.clone());

            // Set to published
            let payload_hash = rand_vec::<24>();
            let post_commit_data = rand_vec::<24>();
            conn.set_group_intent_published(
                intent.id,
                &payload_hash,
                Some(post_commit_data.clone()),
                None,
                1,
            )
            .unwrap();

            intent = conn.fetch(&intent.id).unwrap().unwrap();
            assert_eq!(intent.state, IntentState::Published);
            assert_eq!(intent.payload_hash, Some(payload_hash.clone()));
            assert_eq!(intent.post_commit_data, Some(post_commit_data.clone()));

            conn.set_group_intent_committed(intent.id, Cursor::default())
                .unwrap();
            // Refresh from the DB
            intent = conn.fetch(&intent.id).unwrap().unwrap();
            assert_eq!(intent.state, IntentState::Committed);
            // Make sure we haven't lost the payload hash
            assert_eq!(intent.payload_hash, Some(payload_hash.clone()));
        })
        .await
    }

    #[xmtp_common::test]
    async fn test_republish_state_transition() {
        let group_id = rand_vec::<24>();

        with_connection(|conn| {
            insert_group(conn, group_id.clone());

            // Store the intent
            NewGroupIntent::new(
                IntentKind::UpdateGroupMembership,
                group_id.clone(),
                rand_vec::<24>(),
                false,
            )
            .store(conn)
            .unwrap();

            let mut intent = find_first_intent(conn, group_id.clone());

            // Set to published
            let payload_hash = rand_vec::<24>();
            let post_commit_data = rand_vec::<24>();
            conn.set_group_intent_published(
                intent.id,
                &payload_hash,
                Some(post_commit_data.clone()),
                None,
                1,
            )
            .unwrap();

            intent = conn.fetch(&intent.id).unwrap().unwrap();
            assert_eq!(intent.state, IntentState::Published);
            assert_eq!(intent.payload_hash, Some(payload_hash.clone()));

            // Now revert back to ToPublish
            conn.set_group_intent_to_publish(intent.id).unwrap();
            intent = conn.fetch(&intent.id).unwrap().unwrap();
            assert_eq!(intent.state, IntentState::ToPublish);
            assert!(intent.payload_hash.is_none());
            assert!(intent.post_commit_data.is_none());
        })
        .await
    }

    #[xmtp_common::test]
    async fn test_invalid_state_transition() {
        let group_id = rand_vec::<24>();

        with_connection(|conn| {
            insert_group(conn, group_id.clone());

            // Store the intent
            NewGroupIntent::new(
                IntentKind::UpdateGroupMembership,
                group_id.clone(),
                rand_vec::<24>(),
                false,
            )
            .store(conn)
            .unwrap();

            let intent = find_first_intent(conn, group_id.clone());

            let commit_result = conn.set_group_intent_committed(intent.id, Cursor::default());
            assert!(commit_result.is_err());
            assert!(matches!(
                commit_result.err().unwrap(),
                StorageError::NotFound(_)
            ));

            let to_publish_result = conn.set_group_intent_to_publish(intent.id);
            assert!(to_publish_result.is_err());
            assert!(matches!(
                to_publish_result.err().unwrap(),
                StorageError::NotFound(_)
            ));
        })
        .await
    }

    #[xmtp_common::test]
    async fn test_increment_publish_attempts() {
        let group_id = rand_vec::<24>();
        with_connection(|conn| {
            insert_group(conn, group_id.clone());
            NewGroupIntent::new(
                IntentKind::UpdateGroupMembership,
                group_id.clone(),
                rand_vec::<24>(),
                false,
            )
            .store(conn)
            .unwrap();

            let mut intent = find_first_intent(conn, group_id.clone());
            assert_eq!(intent.publish_attempts, 0);
            conn.increment_intent_publish_attempt_count(intent.id)
                .unwrap();
            intent = find_first_intent(conn, group_id.clone());
            assert_eq!(intent.publish_attempts, 1);
            conn.increment_intent_publish_attempt_count(intent.id)
                .unwrap();
            intent = find_first_intent(conn, group_id.clone());
            assert_eq!(intent.publish_attempts, 2);
        })
        .await
    }

    fn make_membership_change_message<C: ConnectionExt>(
        conn: &DbConnection<C>,
        group_id: Vec<u8>,
        sequence_id: i64,
        originator_id: i64,
        published_in_epoch: Option<i64>,
    ) {
        use crate::group_message::{
            ContentType, DeliveryStatus, GroupMessageKind, StoredGroupMessage,
        };

        StoredGroupMessage {
            id: rand_vec::<24>(),
            group_id,
            decrypted_message_bytes: vec![],
            sent_at_ns: 0,
            kind: GroupMessageKind::MembershipChange,
            sender_installation_id: vec![],
            sender_inbox_id: String::new(),
            delivery_status: DeliveryStatus::Published,
            content_type: ContentType::GroupMembershipChange,
            version_major: 0,
            version_minor: 0,
            authority_id: String::new(),
            reference_id: None,
            inserted_at_ns: 0,
            expire_at_ns: None,
            sequence_id,
            originator_id,
            published_in_epoch,
        }
        .store(conn)
        .unwrap()
    }

    #[derive(Debug, Clone, Hash)]
    struct IntentWithDependency {
        payload_hash: PayloadHash,
        intent_epoch: usize,
        dependency: MessageDep,
    }

    #[derive(Clone, Debug, Hash, Copy, PartialEq, Eq)]
    struct MessageDep {
        cursor: Cursor,
        epoch: usize,
    }

    prop_compose! {
        // ensures epoch is always unique
       fn messages(length: usize)(epochs in collection::hash_set(1usize..50usize, 1..length), oid in 0u32..100, sid in 0u64..1000) -> Vec<MessageDep> {
            epochs.into_iter().map(|epoch| {
                MessageDep {
                    cursor: Cursor {
                        sequence_id: sid,
                        originator_id: oid
                    },
                    epoch
                }
            }).collect()
        }
    }

    prop_compose! {
        fn intent_data_vec(msg_len: usize, intents: usize)
            (
                msgs in messages(msg_len),
                indices in prop::collection::vec(any::<prop::sample::Index>(), 0..intents)
            ) -> Vec<IntentWithDependency> {
            let mut deps = Vec::new();
            for index in indices {
                let dependency = index.get(&msgs);
                deps.push(IntentWithDependency {
                    payload_hash: rand_vec::<24>().into(),
                    intent_epoch: dependency.epoch + 1,
                    dependency: *dependency
                })
            }
            deps
        }
    }

    #[xmtp_common::test]
    fn proptest_find_dependant_commits() {
        use futures::FutureExt;

        proptest!(|(intents_data in intent_data_vec(10, 20))| {
            let group_id = rand_vec::<16>();
            with_connection(|conn| {
                insert_group(conn, group_id.clone());

                let payload_hashes: Vec<PayloadHash> = intents_data.iter().cloned().map(|i| i.payload_hash).collect();
                intents_data.iter().map(|d| d.dependency).unique().for_each(|d| {
                    make_membership_change_message(
                        conn,
                        group_id.clone(),
                        d.cursor.sequence_id as i64,
                        d.cursor.originator_id as i64,
                        Some(d.epoch as i64),
                    );
                });

                for intent_data in &intents_data {
                    // Create and publish intent
                    let intent = NewGroupIntent::new(
                        IntentKind::SendMessage,
                        group_id.clone(),
                        rand_vec::<24>(),
                        false,
                    );
                    let stored_intent = conn.insert_group_intent(intent).unwrap();
                    conn.set_group_intent_published(
                        stored_intent.id,
                        &intent_data.payload_hash,
                        None,
                        None,
                        intent_data.intent_epoch as i64
                    ).unwrap();
                }
                // Query all dependencies at once
                let results = conn.find_dependant_commits(&payload_hashes).unwrap();

                prop_assert_eq!(results.len(), intents_data.len());
                Ok(())
            }).now_or_never();
        });
    }

    #[xmtp_common::test]
    async fn test_find_dependant_commits_none_found() {
        let group_id = rand_vec::<24>();

        with_connection(|conn| {
            insert_group(conn, group_id.clone());

            // Create and publish an intent
            let intent = NewGroupIntent::new(
                IntentKind::SendMessage,
                group_id.clone(),
                rand_vec::<24>(),
                false,
            );
            let stored_intent = conn.insert_group_intent(intent).unwrap();
            let payload_hash = rand_vec::<24>();
            conn.set_group_intent_published(
                stored_intent.id,
                &payload_hash,
                None,
                None,
                5, // Published in epoch 5
            )
            .unwrap();
            let result = conn.find_dependant_commits(&[&payload_hash]).unwrap();
            assert!(result.is_empty());
        })
        .await
    }

    #[xmtp_common::test]
    async fn test_find_dependant_commits_multiple_dependencies_error() {
        let group_id = rand_vec::<24>();

        with_connection(|conn| {
            insert_group(conn, group_id.clone());

            // Create and publish an intent in epoch 5
            let intent = NewGroupIntent::new(
                IntentKind::SendMessage,
                group_id.clone(),
                rand_vec::<24>(),
                false,
            );
            let stored_intent = conn.insert_group_intent(intent).unwrap();
            let payload_hash = rand_vec::<24>();
            conn.set_group_intent_published(
                stored_intent.id,
                &payload_hash,
                None,
                None,
                5, // Published in epoch 5
            )
            .unwrap();

            // Create TWO membership change messages in epoch 4 (the dependency epoch)
            make_membership_change_message(conn, group_id.clone(), 100, 1, Some(4));
            make_membership_change_message(conn, group_id.clone(), 200, 2, Some(4));

            // Should return an error due to multiple dependencies
            let result = conn.find_dependant_commits(&[&payload_hash]);

            assert!(result.is_err());
            match result.unwrap_err() {
                StorageError::GroupIntent(GroupIntentError::MoreThanTwoDependencies {
                    payload_hash: hash,
                    ..
                }) => {
                    assert_eq!(hash, payload_hash.into());
                }
                other => panic!("Expected MoreThanTwoDependencies error, got: {:?}", other),
            }
        })
        .await
    }
}
