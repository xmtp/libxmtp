use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Integer,
    sqlite::Sqlite,
};

use super::{
    db_connection::DbConnection,
    group,
    schema::{group_intents, group_intents::dsl},
};
use crate::{impl_fetch, impl_store, storage::StorageError, Delete};

pub type ID = i32;

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, AsExpression, FromSqlRow)]
#[diesel(sql_type = Integer)]
pub enum IntentKind {
    SendMessage = 1,
    KeyUpdate = 2,
    MetadataUpdate = 3,
    UpdateGroupMembership = 4,
    UpdateAdminList = 5,
}

impl std::fmt::Display for IntentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let description = match self {
            IntentKind::SendMessage => "SendMessage",
            IntentKind::KeyUpdate => "KeyUpdate",
            IntentKind::MetadataUpdate => "MetadataUpdate",
            IntentKind::UpdateGroupMembership => "UpdateGroupMembership",
            IntentKind::UpdateAdminList => "UpdateAdminList",
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
}

#[derive(Queryable, Identifiable, Debug, PartialEq, Clone)]
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
}

impl_fetch!(StoredGroupIntent, group_intents, ID);

impl Delete<StoredGroupIntent> for DbConnection {
    type Key = ID;
    fn delete(&self, key: ID) -> Result<usize, StorageError> {
        Ok(self
            .raw_query(|raw_conn| diesel::delete(dsl::group_intents.find(key)).execute(raw_conn))?)
    }
}

#[derive(Insertable, Debug, PartialEq, Clone)]
#[diesel(table_name = group_intents)]
pub struct NewGroupIntent {
    pub kind: IntentKind,
    pub group_id: Vec<u8>,
    pub data: Vec<u8>,
    pub state: IntentState,
}

impl_store!(NewGroupIntent, group_intents);

impl NewGroupIntent {
    pub fn new(kind: IntentKind, group_id: Vec<u8>, data: Vec<u8>) -> Self {
        Self {
            kind,
            group_id,
            data,
            state: IntentState::ToPublish,
        }
    }
}

impl DbConnection {
    #[tracing::instrument(level = "trace", skip(self))]
    pub fn insert_group_intent(
        &self,
        to_save: NewGroupIntent,
    ) -> Result<StoredGroupIntent, StorageError> {
        Ok(self.raw_query(|conn| {
            diesel::insert_into(dsl::group_intents)
                .values(to_save)
                .get_result(conn)
        })?)
    }

    // Query for group_intents by group_id, optionally filtering by state and kind
    #[tracing::instrument(level = "trace", skip(self))]
    pub fn find_group_intents(
        &self,
        group_id: Vec<u8>,
        allowed_states: Option<Vec<IntentState>>,
        allowed_kinds: Option<Vec<IntentKind>>,
    ) -> Result<Vec<StoredGroupIntent>, StorageError> {
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

        Ok(self.raw_query(|conn| query.load::<StoredGroupIntent>(conn))?)
    }

    // Set the intent with the given ID to `Published` and set the payload hash. Optionally add
    // `post_commit_data`
    pub fn set_group_intent_published(
        &self,
        intent_id: ID,
        payload_hash: Vec<u8>,
        post_commit_data: Option<Vec<u8>>,
    ) -> Result<(), StorageError> {
        let res = self.raw_query(|conn| {
            diesel::update(dsl::group_intents)
                .filter(dsl::id.eq(intent_id))
                // State machine requires that the only valid state transition to Published is from
                // ToPublish
                .filter(dsl::state.eq(IntentState::ToPublish))
                .set((
                    dsl::state.eq(IntentState::Published),
                    dsl::payload_hash.eq(payload_hash),
                    dsl::post_commit_data.eq(post_commit_data),
                ))
                .execute(conn)
        })?;

        match res {
            // If nothing matched the query, return an error. Either ID or state was wrong
            0 => Err(StorageError::NotFound(format!(
                "ToPublish intent {intent_id} for publish"
            ))),
            _ => Ok(()),
        }
    }

    // Set the intent with the given ID to `Committed`
    pub fn set_group_intent_committed(&self, intent_id: ID) -> Result<(), StorageError> {
        let res = self.raw_query(|conn| {
            diesel::update(dsl::group_intents)
                .filter(dsl::id.eq(intent_id))
                // State machine requires that the only valid state transition to Committed is from
                // Published
                .filter(dsl::state.eq(IntentState::Published))
                .set(dsl::state.eq(IntentState::Committed))
                .execute(conn)
        })?;

        match res {
            // If nothing matched the query, return an error. Either ID or state was wrong
            0 => Err(StorageError::NotFound(format!(
                "Published intent {intent_id} for commit"
            ))),
            _ => Ok(()),
        }
    }

    // Set the intent with the given ID to `ToPublish`. Wipe any values for `payload_hash` and
    // `post_commit_data`
    pub fn set_group_intent_to_publish(&self, intent_id: ID) -> Result<(), StorageError> {
        let res = self.raw_query(|conn| {
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
                ))
                .execute(conn)
        })?;

        match res {
            // If nothing matched the query, return an error. Either ID or state was wrong
            0 => Err(StorageError::NotFound(format!(
                "Published intent {intent_id} for ToPublish"
            ))),
            _ => Ok(()),
        }
    }

    /// Set the intent with the given ID to `Error`
    #[tracing::instrument(level = "trace", skip(self))]
    pub fn set_group_intent_error(&self, intent_id: ID) -> Result<(), StorageError> {
        let res = self.raw_query(|conn| {
            diesel::update(dsl::group_intents)
                .filter(dsl::id.eq(intent_id))
                .set(dsl::state.eq(IntentState::Error))
                .execute(conn)
        })?;

        match res {
            // If nothing matched the query, return an error. Either ID or state was wrong
            0 => Err(StorageError::NotFound(format!(
                "state for intent {intent_id}"
            ))),
            _ => Ok(()),
        }
    }

    // Simple lookup of intents by payload hash, meant to be used when processing messages off the
    // network
    pub fn find_group_intent_by_payload_hash(
        &self,
        payload_hash: Vec<u8>,
    ) -> Result<Option<StoredGroupIntent>, StorageError> {
        let result = self.raw_query(|conn| {
            dsl::group_intents
                .filter(dsl::payload_hash.eq(payload_hash))
                .first::<StoredGroupIntent>(conn)
                .optional()
        })?;

        Ok(result)
    }

    pub fn increment_intent_publish_attempt_count(
        &self,
        intent_id: ID,
    ) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::update(dsl::group_intents)
                .filter(dsl::id.eq(intent_id))
                .set(dsl::publish_attempts.eq(dsl::publish_attempts + 1))
                .execute(conn)
        })?;

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
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        storage::encrypted_store::{
            group::{GroupMembershipState, StoredGroup},
            tests::with_connection,
        },
        utils::test::rand_vec,
        Fetch, Store,
    };

    fn insert_group(conn: &DbConnection, group_id: Vec<u8>) {
        let group = StoredGroup::new(
            group_id,
            100,
            GroupMembershipState::Allowed,
            "placeholder_address".to_string(),
        );
        group.store(conn).unwrap();
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
            }
        }
    }

    fn find_first_intent(conn: &DbConnection, group_id: group::ID) -> StoredGroupIntent {
        conn.raw_query(|raw_conn| {
            dsl::group_intents
                .filter(dsl::group_id.eq(group_id))
                .first(raw_conn)
        })
        .unwrap()
    }

    #[test]
    fn test_store_and_fetch() {
        let group_id = rand_vec();
        let data = rand_vec();
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

    #[test]
    fn test_query() {
        let group_id = rand_vec();

        let test_intents: Vec<NewGroupIntent> = vec![
            NewGroupIntent::new_test(
                IntentKind::UpdateGroupMembership,
                group_id.clone(),
                rand_vec(),
                IntentState::ToPublish,
            ),
            NewGroupIntent::new_test(
                IntentKind::KeyUpdate,
                group_id.clone(),
                rand_vec(),
                IntentState::Published,
            ),
            NewGroupIntent::new_test(
                IntentKind::KeyUpdate,
                group_id.clone(),
                rand_vec(),
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

    #[test]
    fn find_by_payload_hash() {
        let group_id = rand_vec();

        with_connection(|conn| {
            insert_group(conn, group_id.clone());

            // Store the intent
            NewGroupIntent::new(
                IntentKind::UpdateGroupMembership,
                group_id.clone(),
                rand_vec(),
            )
            .store(conn)
            .unwrap();

            // Find the intent with the ID populated
            let intent = find_first_intent(conn, group_id.clone());

            // Set the payload hash
            let payload_hash = rand_vec();
            let post_commit_data = rand_vec();
            conn.set_group_intent_published(
                intent.id,
                payload_hash.clone(),
                Some(post_commit_data.clone()),
            )
            .unwrap();

            let find_result = conn
                .find_group_intent_by_payload_hash(payload_hash)
                .unwrap()
                .unwrap();

            assert_eq!(find_result.id, intent.id);
        })
    }

    #[test]
    fn test_happy_path_state_transitions() {
        let group_id = rand_vec();

        with_connection(|conn| {
            insert_group(conn, group_id.clone());

            // Store the intent
            NewGroupIntent::new(
                IntentKind::UpdateGroupMembership,
                group_id.clone(),
                rand_vec(),
            )
            .store(conn)
            .unwrap();

            let mut intent = find_first_intent(conn, group_id.clone());

            // Set to published
            let payload_hash = rand_vec();
            let post_commit_data = rand_vec();
            conn.set_group_intent_published(
                intent.id,
                payload_hash.clone(),
                Some(post_commit_data.clone()),
            )
            .unwrap();

            intent = conn.fetch(&intent.id).unwrap().unwrap();
            assert_eq!(intent.state, IntentState::Published);
            assert_eq!(intent.payload_hash, Some(payload_hash.clone()));
            assert_eq!(intent.post_commit_data, Some(post_commit_data.clone()));

            conn.set_group_intent_committed(intent.id).unwrap();
            // Refresh from the DB
            intent = conn.fetch(&intent.id).unwrap().unwrap();
            assert_eq!(intent.state, IntentState::Committed);
            // Make sure we haven't lost the payload hash
            assert_eq!(intent.payload_hash, Some(payload_hash.clone()));
        })
    }

    #[test]
    fn test_republish_state_transition() {
        let group_id = rand_vec();

        with_connection(|conn| {
            insert_group(conn, group_id.clone());

            // Store the intent
            NewGroupIntent::new(
                IntentKind::UpdateGroupMembership,
                group_id.clone(),
                rand_vec(),
            )
            .store(conn)
            .unwrap();

            let mut intent = find_first_intent(conn, group_id.clone());

            // Set to published
            let payload_hash = rand_vec();
            let post_commit_data = rand_vec();
            conn.set_group_intent_published(
                intent.id,
                payload_hash.clone(),
                Some(post_commit_data.clone()),
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

    #[test]
    fn test_invalid_state_transition() {
        let group_id = rand_vec();

        with_connection(|conn| {
            insert_group(conn, group_id.clone());

            // Store the intent
            NewGroupIntent::new(
                IntentKind::UpdateGroupMembership,
                group_id.clone(),
                rand_vec(),
            )
            .store(conn)
            .unwrap();

            let intent = find_first_intent(conn, group_id.clone());

            let commit_result = conn.set_group_intent_committed(intent.id);
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

    #[test]
    fn test_increment_publish_attempts() {
        let group_id = rand_vec();
        with_connection(|conn| {
            insert_group(conn, group_id.clone());
            NewGroupIntent::new(
                IntentKind::UpdateGroupMembership,
                group_id.clone(),
                rand_vec(),
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
}
