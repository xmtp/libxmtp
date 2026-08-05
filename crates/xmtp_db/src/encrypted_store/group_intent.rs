use std::collections::HashMap;

use derive_builder::Builder;
#[cfg(feature = "sync")]
use diesel::{
    connection::DefaultLoadingMode, deserialize::FromSqlRow, expression::AsExpression, prelude::*,
    sql_types::Integer,
};
#[cfg(feature = "sync")]
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use xmtp_common::fmt;
use xmtp_proto::types::{Cursor, GroupId};

#[cfg(feature = "sync")]
use super::{
    ConnectionExt,
    db_connection::DbConnection,
    schema::group_intents::{self, dsl},
};
#[cfg(feature = "sync")]
use crate::{Delete, impl_fetch, impl_store};
use crate::{NotFound, StorageError, group_message::QueryGroupMessage};

mod error;
mod types;
pub use error::*;
pub use types::*;

pub type ID = i32;

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::EnumIter)]
#[cfg_attr(feature = "sync", derive(AsExpression, FromSqlRow))]
#[cfg_attr(feature = "sync", diesel(sql_type = Integer))]
pub enum IntentKind {
    SendMessage = 1,
    KeyUpdate = 2,
    MetadataUpdate = 3,
    UpdateGroupMembership = 4,
    UpdateAdminList = 5,
    UpdatePermission = 6,
    ReaddInstallations = 7,
    ProposeMemberUpdate = 8,
    ProposeGroupContextExtensions = 9,
    CommitPendingProposals = 10,
    /// One-time bootstrap commit that flips a group from the legacy
    /// GroupContextExtensions-backed metadata layout onto the AppData
    /// dictionary. Distinct from [`Self::ProposeGroupContextExtensions`]
    /// because the payload shape is different (it bundles a GCE proposal
    /// with a fan-out of `AppDataUpdate` proposals) and because the
    /// dispatch path in `mls_sync` needs an explicit marker rather than
    /// sniffing the extension-set shape.
    #[doc(alias = "AppData migration")]
    BootstrapMigration = 11,
    /// Generic AppData component write. The intent payload carries a
    /// `(component_id, AppDataUpdateOp)` pair where `AppDataUpdateOp` is
    /// either `Replace(bytes)` (full-replace components — Bytes / String
    /// types) or `DeltaWithBase { pre, post }` (TlsMap / TlsSet types,
    /// where the handler computes the residual delta at commit time from
    /// the current state, the pre value, and the post value).
    ///
    /// Replaces the proliferation of per-component IntentKinds. Existing
    /// typed intents (`UpdateAdminList`, `UpdatePermission`,
    /// `MetadataUpdate`) are not migrated by the introducing PR — they
    /// continue to work, and a follow-on can fold them in.
    AppDataUpdate = 12,
}

impl IntentKind {
    /// Every kind this build knows how to deserialize, as a lazy
    /// iterator — collect into a `Vec` only where a filter needs one.
    /// Production queries pass these as the `allowed_kinds` filter so
    /// rows written by a NEWER build (which may use discriminants this
    /// build has no variant for) are excluded in SQL instead of
    /// poisoning the whole `load()` — `FromSql` errors on unknown
    /// discriminants, and one such row would otherwise wedge every
    /// intent query for the group after an app downgrade. Unknown-kind
    /// rows stay untouched in the table and resume processing when the
    /// app is upgraded again.
    ///
    /// Exhaustive by construction: `strum::EnumIter` generates the
    /// iteration over every variant, so a newly added `IntentKind` is
    /// included automatically — which is exactly right here, since every
    /// variant is, by definition, a kind this build can deserialize.
    pub fn all() -> impl Iterator<Item = IntentKind> {
        use strum::IntoEnumIterator;
        IntentKind::iter()
    }
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
            IntentKind::ProposeMemberUpdate => "ProposeMemberUpdate",
            IntentKind::ProposeGroupContextExtensions => "ProposeGroupContextExtensions",
            IntentKind::CommitPendingProposals => "CommitPendingProposals",
            IntentKind::BootstrapMigration => "BootstrapMigration",
            IntentKind::AppDataUpdate => "AppDataUpdate",
        };
        write!(f, "{}", description)
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "sync", derive(AsExpression, FromSqlRow))]
#[cfg_attr(feature = "sync", diesel(sql_type = Integer))]
pub enum IntentState {
    ToPublish = 1,
    Published = 2,
    Committed = 3,
    Error = 4,
    Processed = 5,
}

#[derive(PartialEq, Clone, xmtp_macro::PgModel)]
#[xmtp(table = "group_intents")]
#[cfg_attr(feature = "sync", derive(Queryable, Identifiable))]
#[cfg_attr(feature = "sync", diesel(table_name = group_intents))]
#[cfg_attr(feature = "sync", diesel(primary_key(id)))]
pub struct StoredGroupIntent {
    pub id: ID,
    pub kind: IntentKind,
    pub group_id: GroupId,
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
            fmt::truncate_hex(hex::encode(self.group_id))
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

#[cfg(feature = "sync")]
impl_fetch!(StoredGroupIntent, group_intents, ID);

#[cfg(feature = "sync")]
impl<C: ConnectionExt> Delete<StoredGroupIntent> for DbConnection<C> {
    type Key = ID;
    fn delete(&self, key: ID) -> Result<usize, StorageError> {
        Ok(self
            .raw_query(|raw_conn| diesel::delete(dsl::group_intents.find(key)).execute(raw_conn))?)
    }
}

/// NewGroupIntent is the data needed to create a new group intent.
/// Do not use this struct directly outside of the storage module.
/// Use the `queue_intent` method on `MlsGroup` instead.
#[derive(Debug, PartialEq, Clone, Builder)]
#[cfg_attr(feature = "sync", derive(Insertable))]
#[cfg_attr(feature = "sync", diesel(table_name = group_intents))]
#[builder(setter(into), build_fn(error = "StorageError"))]
pub struct NewGroupIntent {
    pub kind: IntentKind,
    pub group_id: GroupId,
    pub data: Vec<u8>,
    pub should_push: bool,
    #[builder(default = "IntentState::ToPublish")]
    pub state: IntentState,
}

#[cfg(feature = "sync")]
impl_store!(NewGroupIntent, group_intents);

impl NewGroupIntent {
    pub fn builder() -> NewGroupIntentBuilder {
        NewGroupIntentBuilder::default()
    }

    pub fn new(
        kind: IntentKind,
        group_id: impl Into<GroupId>,
        data: Vec<u8>,
        should_push: bool,
    ) -> Self {
        Self {
            kind,
            group_id: group_id.into(),
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
    ) -> impl std::future::Future<Output = Result<StoredGroupIntent, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    // Query for group_intents by group_id, optionally filtering by state and kind
    fn find_group_intents(
        &self,
        group_id: &[u8],
        allowed_states: Option<Vec<IntentState>>,
        allowed_kinds: Option<Vec<IntentKind>>,
    ) -> impl std::future::Future<Output = Result<Vec<StoredGroupIntent>, crate::ConnectionError>>
    + xmtp_common::MaybeSend;

    // Set the intent with the given ID to `Published` and set the payload hash. Optionally add
    // `post_commit_data`
    fn set_group_intent_published(
        &self,
        intent_id: ID,
        payload_hash: &[u8],
        post_commit_data: Option<Vec<u8>>,
        staged_commit: Option<Vec<u8>>,
        published_in_epoch: i64,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + xmtp_common::MaybeSend;

    // Set the intent with the given ID to `Committed`
    fn set_group_intent_committed(
        &self,
        intent_id: ID,
        cursor: Cursor,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + xmtp_common::MaybeSend;

    // Set the intent with the given ID to `Committed`
    fn set_group_intent_processed(
        &self,
        intent_id: ID,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + xmtp_common::MaybeSend;

    // Set the intent with the given ID to `ToPublish`. Wipe any values for `payload_hash` and
    // `post_commit_data`
    fn set_group_intent_to_publish(
        &self,
        intent_id: ID,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + xmtp_common::MaybeSend;

    /// Set the intent with the given ID to `Error`
    fn set_group_intent_error(
        &self,
        intent_id: ID,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + xmtp_common::MaybeSend;

    // Simple lookup of intents by payload hash, meant to be used when processing messages off the
    // network
    fn find_group_intent_by_payload_hash(
        &self,
        payload_hash: &[u8],
    ) -> impl std::future::Future<Output = Result<Option<StoredGroupIntent>, StorageError>>
    + xmtp_common::MaybeSend;

    /// find the commit message refresh state for each intent payload hash
    fn find_dependant_commits(
        &self,
        payload_hashes: &[&[u8]],
    ) -> impl std::future::Future<
        Output = Result<HashMap<PayloadHash, IntentDependency>, StorageError>,
    > + xmtp_common::MaybeSend;

    fn increment_intent_publish_attempt_count(
        &self,
        intent_id: ID,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + xmtp_common::MaybeSend;

    fn set_group_intent_error_and_fail_msg(
        &self,
        intent: &StoredGroupIntent,
        msg_id: Option<Vec<u8>>,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + xmtp_common::MaybeSend;
}

impl<T> QueryGroupIntent for &T
where
    T: QueryGroupIntent + xmtp_common::MaybeSync,
{
    async fn insert_group_intent(
        &self,
        to_save: NewGroupIntent,
    ) -> Result<StoredGroupIntent, crate::ConnectionError> {
        (**self).insert_group_intent(to_save).await
    }

    async fn find_group_intents(
        &self,
        group_id: &[u8],
        allowed_states: Option<Vec<IntentState>>,
        allowed_kinds: Option<Vec<IntentKind>>,
    ) -> Result<Vec<StoredGroupIntent>, crate::ConnectionError> {
        (**self)
            .find_group_intents(group_id, allowed_states, allowed_kinds)
            .await
    }

    async fn set_group_intent_published(
        &self,
        intent_id: ID,
        payload_hash: &[u8],
        post_commit_data: Option<Vec<u8>>,
        staged_commit: Option<Vec<u8>>,
        published_in_epoch: i64,
    ) -> Result<(), StorageError> {
        (**self)
            .set_group_intent_published(
                intent_id,
                payload_hash,
                post_commit_data,
                staged_commit,
                published_in_epoch,
            )
            .await
    }

    async fn set_group_intent_committed(
        &self,
        intent_id: ID,
        cursor: Cursor,
    ) -> Result<(), StorageError> {
        (**self).set_group_intent_committed(intent_id, cursor).await
    }

    async fn set_group_intent_processed(&self, intent_id: ID) -> Result<(), StorageError> {
        (**self).set_group_intent_processed(intent_id).await
    }

    async fn set_group_intent_to_publish(&self, intent_id: ID) -> Result<(), StorageError> {
        (**self).set_group_intent_to_publish(intent_id).await
    }

    async fn set_group_intent_error(&self, intent_id: ID) -> Result<(), StorageError> {
        (**self).set_group_intent_error(intent_id).await
    }

    async fn find_group_intent_by_payload_hash(
        &self,
        payload_hash: &[u8],
    ) -> Result<Option<StoredGroupIntent>, StorageError> {
        (**self)
            .find_group_intent_by_payload_hash(payload_hash)
            .await
    }

    async fn find_dependant_commits(
        &self,
        payload_hashes: &[&[u8]],
    ) -> Result<HashMap<PayloadHash, IntentDependency>, StorageError> {
        (**self).find_dependant_commits(payload_hashes).await
    }

    async fn increment_intent_publish_attempt_count(
        &self,
        intent_id: ID,
    ) -> Result<(), StorageError> {
        (**self)
            .increment_intent_publish_attempt_count(intent_id)
            .await
    }

    async fn set_group_intent_error_and_fail_msg(
        &self,
        intent: &StoredGroupIntent,
        msg_id: Option<Vec<u8>>,
    ) -> Result<(), StorageError> {
        (**self)
            .set_group_intent_error_and_fail_msg(intent, msg_id)
            .await
    }
}

#[cfg(feature = "sync")]
impl<C: ConnectionExt> QueryGroupIntent for DbConnection<C> {
    #[xmtp_common::db_span]
    async fn insert_group_intent(
        &self,
        to_save: NewGroupIntent,
    ) -> Result<StoredGroupIntent, crate::ConnectionError> {
        self.raw_query(|conn| {
            diesel::insert_into(dsl::group_intents)
                .values(to_save)
                .get_result(conn)
        })
    }

    // Query for group_intents by group_id, optionally filtering by state and kind
    #[xmtp_common::db_span]
    async fn find_group_intents(
        &self,
        group_id: &[u8],
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

        self.raw_query(|conn| query.load::<StoredGroupIntent>(conn))
    }

    // Set the intent with the given ID to `Published` and set the payload hash. Optionally add
    // `post_commit_data`
    #[tracing::instrument(level = "debug", skip(self, payload_hash), fields(id = intent_id, payload_hash = hex::encode(payload_hash)))]
    async fn set_group_intent_published(
        &self,
        intent_id: ID,
        payload_hash: &[u8],
        post_commit_data: Option<Vec<u8>>,
        staged_commit: Option<Vec<u8>>,
        published_in_epoch: i64,
    ) -> Result<(), StorageError> {
        let rows_changed = self.raw_query(|conn| {
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
            let already_published = self.raw_query(|conn| {
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
    #[tracing::instrument(level = "debug", skip(self))]
    async fn set_group_intent_committed(
        &self,
        intent_id: ID,
        cursor: Cursor,
    ) -> Result<(), StorageError> {
        let rows_changed: usize = self.raw_query(|conn| {
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
    #[tracing::instrument(level = "debug", skip(self))]
    async fn set_group_intent_processed(&self, intent_id: ID) -> Result<(), StorageError> {
        let rows_changed = self.raw_query(|conn| {
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
    #[tracing::instrument(level = "debug", skip(self))]
    async fn set_group_intent_to_publish(&self, intent_id: ID) -> Result<(), StorageError> {
        let rows_changed = self.raw_query(|conn| {
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
    #[tracing::instrument(level = "debug", skip(self))]
    async fn set_group_intent_error(&self, intent_id: ID) -> Result<(), StorageError> {
        let rows_changed = self.raw_query(|conn| {
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
    #[xmtp_common::db_span]
    async fn find_group_intent_by_payload_hash(
        &self,
        payload_hash: &[u8],
    ) -> Result<Option<StoredGroupIntent>, StorageError> {
        let result = self.raw_query(|conn| {
            dsl::group_intents
                .filter(dsl::payload_hash.eq(payload_hash))
                .first::<StoredGroupIntent>(conn)
                .optional()
        })?;

        Ok(result)
    }

    /// Find the commit message refresh state for each intent by payload hash.
    /// Returns a map from payload hash to a vector of dependencies (one per originator).
    #[xmtp_common::db_span]
    async fn find_dependant_commits(
        &self,
        payload_hashes: &[&[u8]],
    ) -> Result<HashMap<PayloadHash, IntentDependency>, StorageError> {
        use super::schema::refresh_state;
        use crate::encrypted_store::refresh_state::EntityKind;

        let hashes = payload_hashes
            .iter()
            .map(|h| PayloadHashRef::from(h.as_ref()));

        // Query all dependencies in a single database call
        let map: HashMap<PayloadHash, Vec<IntentDependency>> = self.raw_query(|conn| {
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
                .load_iter::<(Vec<u8>, i64, i32, GroupId), DefaultLoadingMode>(conn)?
                .map_ok(|(hash, sequence_id, originator_id, group_id)| {
                    (
                        PayloadHash::from(hash),
                        IntentDependency {
                            cursor: Cursor::new(sequence_id as u64, originator_id as u32),
                            group_id,
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
                        group_id: d[0].group_id,
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

    #[tracing::instrument(level = "debug", skip(self))]
    async fn increment_intent_publish_attempt_count(
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

    #[tracing::instrument(level = "debug", skip_all, fields(id = %intent.id, kind = %intent.kind, group_id = %intent.group_id))]
    async fn set_group_intent_error_and_fail_msg(
        &self,
        intent: &StoredGroupIntent,
        msg_id: Option<Vec<u8>>,
    ) -> Result<(), StorageError> {
        self.set_group_intent_error(intent.id).await?;
        if let Some(id) = msg_id {
            self.set_delivery_status_to_failed(&id).await?;
        }
        Ok(())
    }
}

crate::impl_sql_int_enum!(IntentKind {
    SendMessage = 1,
    KeyUpdate = 2,
    MetadataUpdate = 3,
    UpdateGroupMembership = 4,
    UpdateAdminList = 5,
    UpdatePermission = 6,
    ReaddInstallations = 7,
    ProposeMemberUpdate = 8,
    ProposeGroupContextExtensions = 9,
    CommitPendingProposals = 10,
    BootstrapMigration = 11,
    AppDataUpdate = 12,
});

crate::impl_sql_int_enum!(IntentState {
    ToPublish = 1,
    Published = 2,
    Committed = 3,
    Error = 4,
    Processed = 5,
});

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        Fetch, Store,
        group::{GroupMembershipState, StoredGroup},
        test_utils::with_connection,
    };
    use xmtp_common::{Generate, rand_vec};

    fn insert_group<C: ConnectionExt>(conn: &DbConnection<C>, group_id: GroupId) {
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
            group_id: GroupId,
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
        group_id: GroupId,
    ) -> StoredGroupIntent {
        conn.raw_query(|raw_conn| {
            dsl::group_intents
                .filter(dsl::group_id.eq(group_id))
                .first(raw_conn)
        })
        .unwrap()
    }

    /// Exhaustiveness of `IntentKind::all()` is guaranteed by
    /// `strum::EnumIter`, which iterates every variant; what it can't
    /// check is the discriminant layout. Pin it here:
    /// `unknown_kind_row_is_excluded_by_kind_filter` derives its future
    /// discriminant as `all().len() + 1`, which is only "beyond every
    /// known variant" while discriminants are exactly 1..=len with no
    /// gaps or duplicates.
    #[xmtp_common::test]
    fn intent_kind_discriminants_are_contiguous() {
        let mut discriminants: Vec<i32> = IntentKind::all().map(|k| k as i32).collect();
        discriminants.sort_unstable();
        let count = discriminants.len();
        assert_eq!(
            discriminants,
            (1..=count as i32).collect::<Vec<_>>(),
            "IntentKind discriminants must be exactly 1..={} with no gaps or duplicates",
            count
        );
    }

    /// Downgrade simulation: a row whose `kind` discriminant this build
    /// doesn't know (written by a future version) must not poison
    /// kind-filtered queries. Unfiltered queries still error — pinned
    /// here so a future change to that behavior is a conscious one.
    #[xmtp_common::test]
    fn unknown_kind_row_is_excluded_by_kind_filter() {
        let group_id = GroupId::generate();

        with_connection(|conn| {
            insert_group(conn, group_id);

            // A known-kind intent this build must keep seeing.
            NewGroupIntent::new_test(
                IntentKind::SendMessage,
                group_id,
                rand_vec::<24>(),
                IntentState::ToPublish,
            )
            .store(conn)
            .unwrap();

            // A future-kind row (discriminant beyond every known
            // variant), inserted raw — exactly what a newer build
            // leaves behind before an app downgrade.
            let future_kind = IntentKind::all().count() as i32 + 1;
            conn.raw_query(|raw_conn| {
                diesel::insert_into(dsl::group_intents)
                    .values((
                        dsl::kind.eq(future_kind),
                        dsl::group_id.eq(group_id),
                        dsl::data.eq(rand_vec::<24>()),
                        dsl::state.eq(IntentState::ToPublish),
                        dsl::publish_attempts.eq(0),
                        dsl::should_push.eq(false),
                    ))
                    .execute(raw_conn)
            })
            .unwrap();

            // Kind-filtered (the production shape): unknown row is
            // excluded in SQL, the known intent still comes back.
            let intents = conn
                .find_group_intents(
                    group_id,
                    Some(vec![IntentState::ToPublish]),
                    Some(IntentKind::all().collect()),
                )
                .unwrap();
            assert_eq!(intents.len(), 1);
            assert_eq!(intents[0].kind, IntentKind::SendMessage);

            // Unfiltered: the unknown discriminant fails row
            // deserialization and poisons the whole query.
            assert!(
                conn.find_group_intents(group_id, Some(vec![IntentState::ToPublish]), None)
                    .is_err(),
                "unfiltered query should surface the FromSql error for unknown kinds"
            );
        })
    }

    #[xmtp_common::test]
    fn test_store_and_fetch() {
        let group_id = GroupId::generate();
        let data = rand_vec::<24>();
        let kind = IntentKind::UpdateGroupMembership;
        let state = IntentState::ToPublish;

        let to_insert = NewGroupIntent::new_test(kind, group_id, data.clone(), state);

        with_connection(|conn| {
            // Group needs to exist or FK constraint will fail
            insert_group(conn, group_id);

            to_insert.store(conn).unwrap();

            let results = conn
                .find_group_intents(group_id, Some(vec![IntentState::ToPublish]), None)
                .unwrap();

            assert_eq!(results.len(), 1);
            assert_eq!(results[0].kind, kind);
            assert_eq!(results[0].data, data);
            assert_eq!(results[0].group_id.as_slice(), group_id.as_slice());

            let id = results[0].id;

            let fetched: StoredGroupIntent = conn.fetch(&id).unwrap().unwrap();

            assert_eq!(fetched.id, id);
        })
    }

    #[xmtp_common::test]
    fn test_query() {
        let group_id = GroupId::generate();

        let test_intents: Vec<NewGroupIntent> = vec![
            NewGroupIntent::new_test(
                IntentKind::UpdateGroupMembership,
                group_id,
                rand_vec::<24>(),
                IntentState::ToPublish,
            ),
            NewGroupIntent::new_test(
                IntentKind::KeyUpdate,
                group_id,
                rand_vec::<24>(),
                IntentState::Published,
            ),
            NewGroupIntent::new_test(
                IntentKind::KeyUpdate,
                group_id,
                rand_vec::<24>(),
                IntentState::Committed,
            ),
        ];

        with_connection(|conn| {
            // Group needs to exist or FK constraint will fail
            insert_group(conn, group_id);

            for case in test_intents {
                case.store(conn).unwrap();
            }

            // Can query for multiple states
            let mut results = conn
                .find_group_intents(
                    group_id,
                    Some(vec![IntentState::ToPublish, IntentState::Published]),
                    None,
                )
                .unwrap();

            assert_eq!(results.len(), 2);

            // Can query by kind
            results = conn
                .find_group_intents(group_id, None, Some(vec![IntentKind::KeyUpdate]))
                .unwrap();
            assert_eq!(results.len(), 2);

            // Can query by kind and state
            results = conn
                .find_group_intents(
                    group_id,
                    Some(vec![IntentState::Committed]),
                    Some(vec![IntentKind::KeyUpdate]),
                )
                .unwrap();

            assert_eq!(results.len(), 1);

            // Can get no results
            results = conn
                .find_group_intents(
                    group_id,
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
        let group_id = GroupId::generate();

        with_connection(|conn| {
            insert_group(conn, group_id);

            // Store the intent
            NewGroupIntent::new(
                IntentKind::UpdateGroupMembership,
                group_id,
                rand_vec::<24>(),
                false,
            )
            .store(conn)
            .unwrap();

            // Find the intent with the ID populated
            let intent = find_first_intent(conn, group_id);

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
        let group_id = GroupId::generate();

        with_connection(|conn| {
            insert_group(conn, group_id);

            // Store the intent
            NewGroupIntent::new(
                IntentKind::UpdateGroupMembership,
                group_id,
                rand_vec::<24>(),
                false,
            )
            .store(conn)
            .unwrap();

            let mut intent = find_first_intent(conn, group_id);

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
        let group_id = GroupId::generate();

        with_connection(|conn| {
            insert_group(conn, group_id);

            // Store the intent
            NewGroupIntent::new(
                IntentKind::UpdateGroupMembership,
                group_id,
                rand_vec::<24>(),
                false,
            )
            .store(conn)
            .unwrap();

            let mut intent = find_first_intent(conn, group_id);

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
        let group_id = GroupId::generate();

        with_connection(|conn| {
            insert_group(conn, group_id);

            // Store the intent
            NewGroupIntent::new(
                IntentKind::UpdateGroupMembership,
                group_id,
                rand_vec::<24>(),
                false,
            )
            .store(conn)
            .unwrap();

            let intent = find_first_intent(conn, group_id);

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
        let group_id = GroupId::generate();
        with_connection(|conn| {
            insert_group(conn, group_id);
            NewGroupIntent::new(
                IntentKind::UpdateGroupMembership,
                group_id,
                rand_vec::<24>(),
                false,
            )
            .store(conn)
            .unwrap();

            let mut intent = find_first_intent(conn, group_id);
            assert_eq!(intent.publish_attempts, 0);
            conn.increment_intent_publish_attempt_count(intent.id)
                .unwrap();
            intent = find_first_intent(conn, group_id);
            assert_eq!(intent.publish_attempts, 1);
            conn.increment_intent_publish_attempt_count(intent.id)
                .unwrap();
            intent = find_first_intent(conn, group_id);
            assert_eq!(intent.publish_attempts, 2);
        })
    }
    #[xmtp_common::test]
    fn test_find_dependant_commits() {
        use crate::encrypted_store::refresh_state::{EntityKind, QueryRefreshState};

        let group_id = GroupId::generate();
        let payload_hash1 = rand_vec::<24>();
        let payload_hash2 = rand_vec::<24>();

        with_connection(|conn| {
            insert_group(conn, group_id);
            NewGroupIntent::new(IntentKind::SendMessage, group_id, rand_vec::<24>(), false)
                .store(conn)
                .unwrap();

            let intent1 = find_first_intent(conn, group_id);
            conn.set_group_intent_published(intent1.id, &payload_hash1, None, None, 1)
                .unwrap();

            NewGroupIntent::new(IntentKind::KeyUpdate, group_id, rand_vec::<24>(), false)
                .store(conn)
                .unwrap();
            let intents = conn.find_group_intents(group_id, None, None).unwrap();
            let intent2 = intents.iter().find(|i| i.id != intent1.id).unwrap();
            conn.set_group_intent_published(intent2.id, &payload_hash2, None, None, 1)
                .unwrap();

            conn.update_cursor(group_id, EntityKind::CommitMessage, Cursor::new(100, 42u32))
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

    #[xmtp_common::test]
    fn bootstrap_migration_intent_round_trips_through_sql() {
        // Exercises both the i32 → IntentKind::BootstrapMigration arm
        // and the Display impl. Cheap coverage for the new variant
        // that would otherwise sit dead until end-to-end migration tests.
        let group_id = GroupId::generate();
        let data = rand_vec::<24>();
        let kind = IntentKind::BootstrapMigration;
        let to_insert =
            NewGroupIntent::new_test(kind, group_id, data.clone(), IntentState::ToPublish);

        with_connection(|conn| {
            insert_group(conn, group_id);
            to_insert.store(conn).unwrap();

            let results = conn
                .find_group_intents(group_id, Some(vec![IntentState::ToPublish]), None)
                .unwrap();

            assert_eq!(results.len(), 1);
            assert_eq!(results[0].kind, IntentKind::BootstrapMigration);
            assert_eq!(format!("{}", results[0].kind), "BootstrapMigration");
        })
    }
}

/// sqlx backend -- Postgres only. See the note on `QueryGroupVersion`'s impl for
/// why this is gated `not(feature = "sync")`.
#[cfg(all(feature = "async", not(feature = "sync"), not(target_arch = "wasm32")))]
mod pg_impl {
    use super::*;
    use crate::encrypted_store::refresh_state::EntityKind;
    use crate::pg::{PgDb, PgModel};

    /// Arrays of the `#[repr(i32)]` enums have no `PgHasArrayType`, so a filter
    /// list is converted to the integers the column stores before binding.
    fn as_ints<T: Copy>(values: &Option<Vec<T>>, to_int: impl Fn(T) -> i32) -> Option<Vec<i32>> {
        values
            .as_ref()
            .map(|values| values.iter().map(|v| to_int(*v)).collect())
    }

    impl QueryGroupIntent for PgDb {
        async fn insert_group_intent(
            &self,
            to_save: NewGroupIntent,
        ) -> Result<StoredGroupIntent, crate::ConnectionError> {
            let sql = format!(
                "INSERT INTO group_intents (kind, group_id, data, should_push, state) \
                 VALUES ($1, $2, $3, $4, $5) RETURNING {}",
                StoredGroupIntent::select_columns()
            );
            let mut c = self.conn().await?;
            Ok(sqlx::query_as::<_, StoredGroupIntent>(&sql)
                .bind(to_save.kind)
                .bind(to_save.group_id)
                .bind(&to_save.data)
                .bind(to_save.should_push)
                .bind(to_save.state)
                .fetch_one(&mut *c)
                .await?)
        }

        async fn find_group_intents(
            &self,
            group_id: &[u8],
            allowed_states: Option<Vec<IntentState>>,
            allowed_kinds: Option<Vec<IntentKind>>,
        ) -> Result<Vec<StoredGroupIntent>, crate::ConnectionError> {
            let sql = format!(
                "SELECT {} FROM group_intents \
                 WHERE group_id = $1 \
                   AND ($2::int4[] IS NULL OR state = ANY($2)) \
                   AND ($3::int4[] IS NULL OR kind = ANY($3)) \
                 ORDER BY id ASC",
                StoredGroupIntent::select_columns()
            );
            let mut c = self.conn().await?;
            Ok(sqlx::query_as::<_, StoredGroupIntent>(&sql)
                .bind(group_id)
                .bind(as_ints(&allowed_states, |s| s as i32))
                .bind(as_ints(&allowed_kinds, |k| k as i32))
                .fetch_all(&mut *c)
                .await?)
        }

        /// Publishing is idempotent: an intent already past `ToPublish` is left
        /// alone and reported as success, and only a *missing* intent is an
        /// error. Telling those two apart needs the row's existence as well as
        /// the update's outcome, which the sync path gets from a second query.
        ///
        /// Here both come back from one statement. A data-modifying CTE and the
        /// query around it share a snapshot, so the `EXISTS` below sees the row
        /// as it was before the update -- which is exactly what "did this intent
        /// exist at all?" means, and leaves no window for a concurrent delete to
        /// turn an already-published intent into a spurious NotFound.
        async fn set_group_intent_published(
            &self,
            intent_id: ID,
            payload_hash: &[u8],
            post_commit_data: Option<Vec<u8>>,
            staged_commit: Option<Vec<u8>>,
            published_in_epoch: i64,
        ) -> Result<(), StorageError> {
            let mut c = self.conn().await?;
            let (applied, found): (bool, bool) = sqlx::query_as(
                "WITH published AS ( \
                     UPDATE group_intents \
                        SET state = $2, payload_hash = $3, post_commit_data = $4, \
                            staged_commit = $5, published_in_epoch = $6 \
                      WHERE id = $1 AND state = $7 \
                     RETURNING id) \
                 SELECT EXISTS (SELECT 1 FROM published), \
                        EXISTS (SELECT 1 FROM group_intents WHERE id = $1)",
            )
            .bind(intent_id)
            .bind(IntentState::Published)
            .bind(payload_hash)
            .bind(&post_commit_data)
            .bind(&staged_commit)
            .bind(published_in_epoch)
            // The state machine allows only ToPublish -> Published.
            .bind(IntentState::ToPublish)
            .fetch_one(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?;

            if applied || found {
                Ok(())
            } else {
                Err(NotFound::IntentForToPublish(intent_id).into())
            }
        }

        async fn set_group_intent_committed(
            &self,
            intent_id: ID,
            cursor: Cursor,
        ) -> Result<(), StorageError> {
            let mut c = self.conn().await?;
            let rows_changed = sqlx::query(
                "UPDATE group_intents SET state = $2, sequence_id = $3, originator_id = $4 \
                 WHERE id = $1 AND state = $5",
            )
            .bind(intent_id)
            .bind(IntentState::Committed)
            .bind(cursor.sequence_id as i64)
            .bind(cursor.originator_id as i64)
            // The state machine allows only Published -> Committed.
            .bind(IntentState::Published)
            .execute(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?
            .rows_affected();

            // Nothing matched: either the id or the state was wrong.
            if rows_changed == 0 {
                return Err(NotFound::IntentForCommitted(intent_id).into());
            }
            Ok(())
        }

        async fn set_group_intent_processed(&self, intent_id: ID) -> Result<(), StorageError> {
            self.set_intent_state(intent_id, IntentState::Processed)
                .await
        }

        async fn set_group_intent_to_publish(&self, intent_id: ID) -> Result<(), StorageError> {
            let mut c = self.conn().await?;
            let rows_changed = sqlx::query(
                "UPDATE group_intents \
                    SET state = $2, payload_hash = NULL, post_commit_data = NULL, \
                        published_in_epoch = NULL, staged_commit = NULL \
                  WHERE id = $1 AND state = $3",
            )
            .bind(intent_id)
            .bind(IntentState::ToPublish)
            // The state machine allows only Published -> ToPublish.
            .bind(IntentState::Published)
            .execute(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?
            .rows_affected();

            if rows_changed == 0 {
                return Err(NotFound::IntentForPublish(intent_id).into());
            }
            Ok(())
        }

        async fn set_group_intent_error(&self, intent_id: ID) -> Result<(), StorageError> {
            self.set_intent_state(intent_id, IntentState::Error).await
        }

        async fn find_group_intent_by_payload_hash(
            &self,
            payload_hash: &[u8],
        ) -> Result<Option<StoredGroupIntent>, StorageError> {
            let sql = format!(
                "SELECT {} FROM group_intents WHERE payload_hash = $1 LIMIT 1",
                StoredGroupIntent::select_columns()
            );
            let mut c = self.conn().await?;
            Ok(sqlx::query_as::<_, StoredGroupIntent>(&sql)
                .bind(payload_hash)
                .fetch_optional(&mut *c)
                .await
                .map_err(crate::ConnectionError::from)?)
        }

        async fn find_dependant_commits(
            &self,
            payload_hashes: &[&[u8]],
        ) -> Result<HashMap<PayloadHash, IntentDependency>, StorageError> {
            let hashes: Vec<Vec<u8>> = payload_hashes.iter().map(|hash| hash.to_vec()).collect();

            // `payload_hash` decodes as non-null: `NULL = ANY(...)` is NULL, so
            // an unpublished intent can never match.
            let mut c = self.conn().await?;
            let rows: Vec<(Vec<u8>, i64, i32, GroupId)> = sqlx::query_as(
                "SELECT gi.payload_hash, rs.sequence_id, rs.originator_id, gi.group_id \
                 FROM group_intents gi \
                 INNER JOIN refresh_state rs \
                         ON rs.entity_id = gi.group_id AND rs.entity_kind = $2 \
                 WHERE gi.payload_hash = ANY($1::bytea[])",
            )
            .bind(&hashes)
            .bind(EntityKind::CommitMessage)
            .fetch_all(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?;

            let mut grouped: HashMap<PayloadHash, Vec<IntentDependency>> = HashMap::new();
            for (hash, sequence_id, originator_id, group_id) in rows {
                grouped
                    .entry(PayloadHash::from(hash))
                    .or_default()
                    .push(IntentDependency {
                        cursor: Cursor::new(sequence_id as u64, originator_id as u32),
                        group_id,
                    });
            }

            grouped
                .into_iter()
                .map(|(hash, mut dependencies)| {
                    // One commit-message refresh state per group; more than one
                    // means the same payload hash reached two of them.
                    if dependencies.len() > 1 {
                        return Err(GroupIntentError::MoreThanOneDependency {
                            payload_hash: hash.clone(),
                            cursors: dependencies.iter().map(|d| d.cursor).collect(),
                            group_id: dependencies[0].group_id,
                        }
                        .into());
                    }
                    let dependency = dependencies
                        .pop()
                        .ok_or_else(|| GroupIntentError::NoDependencyFound { hash: hash.clone() })
                        .map_err(StorageError::from)?;
                    Ok::<_, StorageError>((hash, dependency))
                })
                .collect()
        }

        async fn increment_intent_publish_attempt_count(
            &self,
            intent_id: ID,
        ) -> Result<(), StorageError> {
            let mut c = self.conn().await?;
            sqlx::query(
                "UPDATE group_intents SET publish_attempts = publish_attempts + 1 WHERE id = $1",
            )
            .bind(intent_id)
            .execute(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?;
            Ok(())
        }

        /// `atomic()` because the two writes are one outcome: an intent marked
        /// failed while its message still looks publishable would be retried
        /// forever.
        async fn set_group_intent_error_and_fail_msg(
            &self,
            intent: &StoredGroupIntent,
            msg_id: Option<Vec<u8>>,
        ) -> Result<(), StorageError> {
            self.atomic(async |db| {
                db.set_group_intent_error(intent.id).await?;
                if let Some(id) = msg_id {
                    db.set_delivery_status_to_failed(&id).await?;
                }
                Ok(())
            })
            .await
        }
    }

    impl PgDb {
        /// The two unguarded state transitions -- any state may move to
        /// `Processed` or `Error`. A miss can only mean the intent is gone.
        async fn set_intent_state(
            &self,
            intent_id: ID,
            state: IntentState,
        ) -> Result<(), StorageError> {
            let mut c = self.conn().await?;
            let rows_changed = sqlx::query("UPDATE group_intents SET state = $2 WHERE id = $1")
                .bind(intent_id)
                .bind(state)
                .execute(&mut *c)
                .await
                .map_err(crate::ConnectionError::from)?
                .rows_affected();

            if rows_changed == 0 {
                return Err(NotFound::IntentById(intent_id).into());
            }
            Ok(())
        }
    }
}
