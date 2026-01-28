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
    Delete, NotFound, StorageError, group_message::QueryGroupMessage, impl_fetch, impl_store,
};

mod error;
mod types;
pub use error::*;
pub use types::*;

pub type ID = i32;

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
    ProposeAddMembers = 8,
    ProposeRemoveMembers = 9,
    ProposeGroupContextExtensions = 10,
    CommitPendingProposals = 11,
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
            IntentKind::ProposeAddMembers => "ProposeAddMembers",
            IntentKind::ProposeRemoveMembers => "ProposeRemoveMembers",
            IntentKind::ProposeGroupContextExtensions => "ProposeGroupContextExtensions",
            IntentKind::CommitPendingProposals => "CommitPendingProposals",
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

#[derive(Queryable, Identifiable, PartialEq, Clone)]
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
    fn find_group_intents<Id: AsRef<[u8]>>(
        &self,
        group_id: Id,
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

    /// find the commit message refresh state for each intent payload hash
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

    fn find_group_intents<Id: AsRef<[u8]>>(
        &self,
        group_id: Id,
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
    #[tracing::instrument(level = "trace", skip(self), fields(group_id = hex::encode(group_id.as_ref())))]
    fn find_group_intents<Id: AsRef<[u8]>>(
        &self,
        group_id: Id,
        allowed_states: Option<Vec<IntentState>>,
        allowed_kinds: Option<Vec<IntentKind>>,
    ) -> Result<Vec<StoredGroupIntent>, crate::ConnectionError> {
        let group_id = group_id.as_ref();
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

    /// Find the commit message refresh state for each intent by payload hash.
    /// Returns a map from payload hash to a vector of dependencies (one per originator).
    fn find_dependant_commits<P: AsRef<[u8]>>(
        &self,
        payload_hashes: &[P],
    ) -> Result<HashMap<PayloadHash, IntentDependency>, StorageError> {
        use super::schema::refresh_state;
        use crate::encrypted_store::refresh_state::EntityKind;

        let hashes = payload_hashes
            .iter()
            .map(|h| PayloadHashRef::from(h.as_ref()));

        // Query all dependencies in a single database call
        let map: HashMap<PayloadHash, Vec<IntentDependency>> = self.raw_query_read(|conn| {
            dsl::group_intents
                .filter(dsl::payload_hash.eq_any(hashes))
                .inner_join(
                    refresh_state::table.on(refresh_state::entity_id
                        .eq(dsl::group_id)
                        .and(refresh_state::entity_kind.eq(EntityKind::CommitMessage))),
                )
                .select((
                    dsl::payload_hash.assume_not_null(),
                    refresh_state::sequence_id,
                    refresh_state::originator_id,
                    dsl::group_id,
                ))
                .load_iter::<(Vec<u8>, i64, i32, Vec<u8>), DefaultLoadingMode>(conn)?
                .map_ok(|(hash, sequence_id, originator_id, group_id)| {
                    (
                        PayloadHash::from(hash),
                        IntentDependency {
                            cursor: Cursor::new(sequence_id as u64, originator_id as u32),
                            group_id: group_id.into(),
                        },
                    )
                })
                .process_results(|iter| iter.into_grouping_map().collect())
        })?;

        let map = map
            .into_iter()
            .map(|(hash, mut d)| {
                if d.len() > 1 {
                    return Err(GroupIntentError::MoreThanOneDependency {
                        payload_hash: hash.clone(),
                        cursors: d.iter().map(|d| d.cursor).collect(),
                        group_id: d[0].group_id.clone(),
                    }
                    .into());
                }

                // this should be impossible since the sql query wouldnt return anything for
                // an empty payload hash.
                let dep = d
                    .pop()
                    .ok_or_else(|| GroupIntentError::NoDependencyFound { hash: hash.clone() })
                    .map_err(StorageError::from)?;
                Ok::<_, StorageError>((hash, dep))
            })
            .try_collect()?;

        Ok(map)
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
            8 => Ok(IntentKind::ProposeAddMembers),
            9 => Ok(IntentKind::ProposeRemoveMembers),
            10 => Ok(IntentKind::ProposeGroupContextExtensions),
            11 => Ok(IntentKind::CommitPendingProposals),
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
    fn test_store_and_fetch() {
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
    }

    #[xmtp_common::test]
    fn test_query() {
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
    }

    #[xmtp_common::test]
    fn find_by_payload_hash() {
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
    }

    #[xmtp_common::test]
    fn test_happy_path_state_transitions() {
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
    }

    #[xmtp_common::test]
    fn test_republish_state_transition() {
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
    }

    #[xmtp_common::test]
    fn test_invalid_state_transition() {
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
    }

    #[xmtp_common::test]
    fn test_increment_publish_attempts() {
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
    }
    #[xmtp_common::test]
    fn test_find_dependant_commits() {
        use crate::encrypted_store::refresh_state::{EntityKind, QueryRefreshState};

        let group_id = rand_vec::<24>();
        let payload_hash1 = rand_vec::<24>();
        let payload_hash2 = rand_vec::<24>();

        with_connection(|conn| {
            insert_group(conn, group_id.clone());
            NewGroupIntent::new(
                IntentKind::SendMessage,
                group_id.clone(),
                rand_vec::<24>(),
                false,
            )
            .store(conn)
            .unwrap();

            let intent1 = find_first_intent(conn, group_id.clone());
            conn.set_group_intent_published(intent1.id, &payload_hash1, None, None, 1)
                .unwrap();

            NewGroupIntent::new(
                IntentKind::KeyUpdate,
                group_id.clone(),
                rand_vec::<24>(),
                false,
            )
            .store(conn)
            .unwrap();
            let intents = conn
                .find_group_intents(group_id.clone(), None, None)
                .unwrap();
            let intent2 = intents.iter().find(|i| i.id != intent1.id).unwrap();
            conn.set_group_intent_published(intent2.id, &payload_hash2, None, None, 1)
                .unwrap();

            conn.update_cursor(
                group_id.clone(),
                EntityKind::CommitMessage,
                Cursor::new(100, 42u32),
            )
            .unwrap();

            let result = conn
                .find_dependant_commits(&[&payload_hash1, &payload_hash2])
                .unwrap();

            assert_eq!(result.len(), 2);
            let dep1 = result
                .get(&PayloadHash::from(payload_hash1.clone()))
                .unwrap();
            assert_eq!(dep1.cursor.sequence_id, 100);
            assert_eq!(dep1.cursor.originator_id, 42);
            assert_eq!(dep1.group_id.as_ref(), &group_id);

            let dep2 = result
                .get(&PayloadHash::from(payload_hash2.clone()))
                .unwrap();
            assert_eq!(dep2.cursor.sequence_id, 100);
            assert_eq!(dep2.cursor.originator_id, 42);
            assert_eq!(dep2.group_id.as_ref(), &group_id);
        })
    }
}
