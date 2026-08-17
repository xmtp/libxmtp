use itertools::Itertools;
use std::collections::HashMap;
use xmtp_api_d14n::protocol::{CursorStore, CursorStoreError};
use xmtp_common::{MaybeSend, MaybeSync};
use xmtp_configuration::Originators;
use xmtp_db::{
    d14n_migration_cutover::QueryMigrationCutover,
    group_intent::IntentDependency,
    icebox::QueryIcebox,
    identity_update::QueryIdentityUpdates,
    prelude::{QueryGroupIntent, QueryRefreshState},
    refresh_state::EntityKind,
};
use xmtp_proto::types::{Cursor, GlobalCursor, OriginatorId, Topic, TopicKind};

#[derive(Clone)]
pub struct SqliteCursorStore<Db> {
    db: Db,
}

impl<Db> SqliteCursorStore<Db> {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

#[xmtp_common::async_trait]
impl<Db> CursorStore for SqliteCursorStore<Db>
where
    Db: QueryRefreshState
        + QueryIdentityUpdates
        + QueryGroupIntent
        + QueryIcebox
        + QueryMigrationCutover
        + MaybeSend
        + MaybeSync,
{
    async fn latest(
        &self,
        topic: &Topic,
        originators: Option<&[&OriginatorId]>,
    ) -> Result<GlobalCursor, CursorStoreError> {
        match topic.kind() {
            TopicKind::WelcomeMessagesV1 => {
                let entities = vec![EntityKind::Welcome];
                self.db
                    .latest_cursor_for_id(topic.identifier(), &entities, originators)
                    .await
                    .map_err(CursorStoreError::other)
            }
            TopicKind::GroupMessagesV1 => {
                let entities = vec![EntityKind::ApplicationMessage, EntityKind::CommitMessage];
                self.db
                    .latest_cursor_for_id(topic.identifier(), &entities, originators)
                    .await
                    .map_err(CursorStoreError::other)
            }
            TopicKind::IdentityUpdatesV1 => {
                let sid = self
                    .db
                    .get_latest_sequence_id_for_inbox(&hex::encode(topic.identifier()))
                    .await
                    .map_err(CursorStoreError::other)?;
                let mut map = GlobalCursor::default();
                map.insert(Originators::INBOX_LOG, sid as u64);
                Ok(map)
            }
            TopicKind::KeyPackagesV1 => Ok(GlobalCursor::default()),
            _ => Err(CursorStoreError::UnhandledTopicKind(topic.kind())),
        }
    }

    async fn latest_for_topics(
        &self,
        topics: &mut (dyn Iterator<Item = &Topic> + Send),
    ) -> Result<HashMap<Topic, GlobalCursor>, CursorStoreError> {
        // Partition topics by kind
        let partitions = topics.into_group_map_by(|t| t.kind());

        // A `for` loop rather than `.map(..).collect::<Result<_, _>>()`: the
        // per-kind work awaits now, and an async block inside `map` would only
        // yield an iterator of futures with no way to `?` through it.
        let mut out: HashMap<Topic, GlobalCursor> = HashMap::new();
        for (kind, topics_of_kind) in partitions {
            match kind {
                TopicKind::WelcomeMessagesV1 => {
                    let identifiers: Vec<_> =
                        topics_of_kind.iter().map(|t| t.identifier()).collect();
                    let mut cursors = self
                        .db
                        .get_last_cursor_for_ids(&identifiers, &[EntityKind::Welcome])
                        .await
                        .map_err(CursorStoreError::other)?;

                    for topic in topics_of_kind {
                        let cursor = cursors.remove(topic.identifier()).unwrap_or_default();
                        out.insert(topic.clone(), cursor);
                    }
                }
                TopicKind::GroupMessagesV1 => {
                    let identifiers: Vec<_> =
                        topics_of_kind.iter().map(|t| t.identifier()).collect();
                    let mut cursors = self
                        .db
                        .get_last_cursor_for_ids(
                            &identifiers,
                            &[EntityKind::ApplicationMessage, EntityKind::CommitMessage],
                        )
                        .await
                        .map_err(CursorStoreError::other)?;

                    for topic in topics_of_kind {
                        let cursor = cursors.remove(topic.identifier()).unwrap_or_default();
                        out.insert(topic.clone(), cursor);
                    }
                }
                TopicKind::IdentityUpdatesV1 => {
                    for topic in topics_of_kind {
                        let sid = self
                            .db
                            .get_latest_sequence_id_for_inbox(&hex::encode(topic.identifier()))
                            .await
                            .map_err(CursorStoreError::other)?;
                        let mut map = GlobalCursor::default();
                        map.insert(Originators::INBOX_LOG, sid as u64);
                        out.insert(topic.clone(), map);
                    }
                }
                TopicKind::KeyPackagesV1 => {
                    for topic in topics_of_kind {
                        out.insert(topic.clone(), GlobalCursor::default());
                    }
                }
                _ => return Err(CursorStoreError::UnhandledTopicKind(kind)),
            }
        }
        Ok(out)
    }

    async fn find_message_dependencies(
        &self,
        hashes: &[&[u8]],
    ) -> Result<HashMap<Vec<u8>, Cursor>, CursorStoreError> {
        let dependencies: HashMap<Vec<u8>, IntentDependency> = self
            .db
            .find_dependant_commits(hashes)
            .await
            .map_err(CursorStoreError::other)?
            .into_iter()
            .map(|(k, v)| (k.into(), v))
            .collect();

        Ok(dependencies
            .into_iter()
            .map(|(h, d)| (h, d.cursor))
            .collect())
    }

    async fn ice(
        &self,
        orphans: Vec<xmtp_proto::types::OrphanedEnvelope>,
    ) -> Result<(), CursorStoreError> {
        self.db
            .ice(orphans)
            .await
            .map_err(CursorStoreError::other)?;
        Ok(())
    }

    async fn resolve_children(
        &self,
        cursors: &[Cursor],
    ) -> Result<Vec<xmtp_proto::types::OrphanedEnvelope>, CursorStoreError> {
        self.db
            .future_dependents(cursors)
            .await
            .map_err(CursorStoreError::other)
    }

    async fn set_cutover_ns(&self, cutover_ns: i64) -> Result<(), CursorStoreError> {
        self.db
            .set_cutover_ns(cutover_ns)
            .await
            .map_err(CursorStoreError::other)
    }

    async fn get_cutover_ns(&self) -> Result<i64, CursorStoreError> {
        let cutover = self
            .db
            .get_migration_cutover()
            .await
            .map_err(CursorStoreError::other)?;
        Ok(cutover.cutover_ns)
    }

    async fn has_migrated(&self) -> Result<bool, CursorStoreError> {
        let cutover = self
            .db
            .get_migration_cutover()
            .await
            .map_err(CursorStoreError::other)?;
        Ok(cutover.has_migrated)
    }

    async fn set_has_migrated(&self, has_migrated: bool) -> Result<(), CursorStoreError> {
        self.db
            .set_has_migrated(has_migrated)
            .await
            .map_err(CursorStoreError::other)
    }

    async fn get_last_checked_ns(&self) -> Result<i64, CursorStoreError> {
        self.db
            .get_last_checked_ns()
            .await
            .map_err(CursorStoreError::other)
    }

    async fn set_last_checked_ns(&self, last_checked_ns: i64) -> Result<(), CursorStoreError> {
        self.db
            .set_last_checked_ns(last_checked_ns)
            .await
            .map_err(CursorStoreError::other)
    }
}
