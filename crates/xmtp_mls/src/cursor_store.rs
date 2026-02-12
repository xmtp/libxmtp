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
    fn lowest_common_cursor(&self, topics: &[&Topic]) -> Result<GlobalCursor, CursorStoreError> {
        self.db
            .lowest_common_cursor(topics)
            .map_err(CursorStoreError::other)
    }

    // temp until reliable streams
    fn lcc_maybe_missing(&self, topics: &[&Topic]) -> Result<GlobalCursor, CursorStoreError> {
        let c = self
            .db
            .lowest_common_cursor_combined(topics)
            .map_err(CursorStoreError::other)?;
        Ok(c)
    }

    fn latest(&self, topic: &Topic) -> Result<GlobalCursor, CursorStoreError> {
        match topic.kind() {
            TopicKind::WelcomeMessagesV1 => {
                let ids = vec![EntityKind::Welcome];
                self.db
                    .latest_cursor_for_id(topic.identifier(), &ids, None)
                    .map_err(CursorStoreError::other)
            }
            TopicKind::GroupMessagesV1 => {
                let ids = vec![EntityKind::ApplicationMessage, EntityKind::CommitMessage];
                self.db
                    .latest_cursor_for_id(topic.identifier(), &ids, None)
                    .map_err(CursorStoreError::other)
            }
            TopicKind::IdentityUpdatesV1 => {
                let sid = self
                    .db
                    .get_latest_sequence_id_for_inbox(&hex::encode(topic.identifier()))
                    .map_err(CursorStoreError::other)?;
                let mut map = GlobalCursor::default();
                map.insert(Originators::INBOX_LOG, sid as u64);
                Ok(map)
            }
            TopicKind::KeyPackagesV1 => Ok(GlobalCursor::default()),
            _ => Err(CursorStoreError::UnhandledTopicKind(topic.kind())),
        }
    }

    fn latest_per_originator(
        &self,
        topic: &Topic,
        originators: &[&OriginatorId],
    ) -> Result<GlobalCursor, CursorStoreError> {
        match topic.kind() {
            TopicKind::WelcomeMessagesV1 => {
                let entities = vec![EntityKind::Welcome];
                self.db
                    .latest_cursor_for_id(topic.identifier(), &entities, Some(originators))
                    .map_err(CursorStoreError::other)
            }
            TopicKind::GroupMessagesV1 => {
                let entities = vec![EntityKind::ApplicationMessage, EntityKind::CommitMessage];
                self.db
                    .latest_cursor_for_id(topic.identifier(), &entities, Some(originators))
                    .map_err(CursorStoreError::other)
            }
            TopicKind::IdentityUpdatesV1 => {
                let sid = self
                    .db
                    .get_latest_sequence_id_for_inbox(&hex::encode(topic.identifier()))
                    .map_err(CursorStoreError::other)?;
                let mut map = GlobalCursor::default();
                map.insert(Originators::INBOX_LOG, sid as u64);
                Ok(map)
            }
            TopicKind::KeyPackagesV1 => Ok(GlobalCursor::default()),
            _ => Err(CursorStoreError::UnhandledTopicKind(topic.kind())),
        }
    }

    fn latest_for_topics(
        &self,
        topics: &mut dyn Iterator<Item = &Topic>,
    ) -> Result<HashMap<Topic, GlobalCursor>, CursorStoreError> {
        // Partition topics by kind
        let partitions = topics.into_group_map_by(|t| t.kind());

        partitions
            .into_iter()
            .map(|(kind, topics_of_kind)| match kind {
                TopicKind::WelcomeMessagesV1 => {
                    let identifiers: Vec<_> =
                        topics_of_kind.iter().map(|t| t.identifier()).collect();
                    let mut cursors = self
                        .db
                        .get_last_cursor_for_ids(&identifiers, &[EntityKind::Welcome])
                        .map_err(CursorStoreError::other)?;

                    Ok(topics_of_kind
                        .into_iter()
                        .map(|topic| {
                            let cursor = cursors.remove(topic.identifier()).unwrap_or_default();
                            (topic.clone(), cursor)
                        })
                        .collect())
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
                        .map_err(CursorStoreError::other)?;

                    Ok(topics_of_kind
                        .into_iter()
                        .map(|topic| {
                            let cursor = cursors.remove(topic.identifier()).unwrap_or_default();
                            (topic.clone(), cursor)
                        })
                        .collect())
                }
                TopicKind::IdentityUpdatesV1 => topics_of_kind
                    .into_iter()
                    .map(|topic| {
                        let sid = self
                            .db
                            .get_latest_sequence_id_for_inbox(&hex::encode(topic.identifier()))
                            .map_err(CursorStoreError::other)?;
                        let mut map = GlobalCursor::default();
                        map.insert(Originators::INBOX_LOG, sid as u64);
                        Ok((topic.clone(), map))
                    })
                    .collect(),
                TopicKind::KeyPackagesV1 => Ok(topics_of_kind
                    .into_iter()
                    .map(|topic| (topic.clone(), GlobalCursor::default()))
                    .collect()),
                _ => Err(CursorStoreError::UnhandledTopicKind(kind)),
            })
            .collect::<Result<Vec<HashMap<Topic, GlobalCursor>>, _>>()
            .map(|results| results.into_iter().flatten().collect())
    }

    fn find_message_dependencies(
        &self,
        hashes: &[&[u8]],
    ) -> Result<HashMap<Vec<u8>, Cursor>, CursorStoreError> {
        let dependencies: HashMap<Vec<u8>, IntentDependency> = self
            .db
            .find_dependant_commits(hashes)
            .map_err(CursorStoreError::other)?
            .into_iter()
            .map(|(k, v)| (k.into(), v))
            .collect();

        Ok(dependencies
            .into_iter()
            .map(|(h, d)| (h, d.cursor))
            .collect())
    }

    fn ice(
        &self,
        orphans: Vec<xmtp_proto::types::OrphanedEnvelope>,
    ) -> Result<(), CursorStoreError> {
        self.db.ice(orphans).map_err(CursorStoreError::other)?;
        Ok(())
    }

    fn resolve_children(
        &self,
        cursors: &[Cursor],
    ) -> Result<Vec<xmtp_proto::types::OrphanedEnvelope>, CursorStoreError> {
        self.db
            .future_dependents(cursors)
            .map_err(CursorStoreError::other)
    }

    fn set_cutover_ns(&self, cutover_ns: i64) -> Result<(), CursorStoreError> {
        self.db
            .set_cutover_ns(cutover_ns)
            .map_err(CursorStoreError::other)
    }

    fn get_cutover_ns(&self) -> Result<i64, CursorStoreError> {
        let cutover = self
            .db
            .get_migration_cutover()
            .map_err(CursorStoreError::other)?;
        Ok(cutover.cutover_ns)
    }

    fn has_migrated(&self) -> Result<bool, CursorStoreError> {
        let cutover = self
            .db
            .get_migration_cutover()
            .map_err(CursorStoreError::other)?;
        Ok(cutover.has_migrated)
    }

    fn set_has_migrated(&self, has_migrated: bool) -> Result<(), CursorStoreError> {
        self.db
            .set_has_migrated(has_migrated)
            .map_err(CursorStoreError::other)
    }

    fn get_last_checked_ns(&self) -> Result<i64, CursorStoreError> {
        self.db
            .get_last_checked_ns()
            .map_err(CursorStoreError::other)
    }

    fn set_last_checked_ns(&self, last_checked_ns: i64) -> Result<(), CursorStoreError> {
        self.db
            .set_last_checked_ns(last_checked_ns)
            .map_err(CursorStoreError::other)
    }
}
