use itertools::Itertools;
use std::collections::HashMap;
use xmtp_api_d14n::protocol::{CursorStore, CursorStoreError};
use xmtp_configuration::Originators;
use xmtp_db::{
    identity_update::QueryIdentityUpdates, prelude::QueryRefreshState, refresh_state::EntityKind,
};
use xmtp_proto::types::{GlobalCursor, OriginatorId, Topic, TopicKind};

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
    Db: QueryRefreshState + QueryIdentityUpdates + Send + Sync,
{
    fn lowest_common_cursor(&self, topics: &[&Topic]) -> Result<GlobalCursor, CursorStoreError> {
        self.db
            .lowest_common_cursor(topics)
            .map_err(|e| CursorStoreError::Other(Box::new(e) as Box<_>))
    }

    // temp until reliable streams
    fn lcc_maybe_missing(&self, topics: &[&Topic]) -> Result<GlobalCursor, CursorStoreError> {
        let c = self
            .db
            .lowest_common_cursor_combined(topics)
            .map_err(|e| CursorStoreError::Other(Box::new(e) as Box<_>))?;
        Ok(c)
    }

    fn latest(&self, topic: &Topic) -> Result<GlobalCursor, CursorStoreError> {
        match topic.kind() {
            TopicKind::WelcomeMessagesV1 => {
                let ids = vec![EntityKind::Welcome];
                self.db
                    .latest_cursor_for_id(topic.identifier(), &ids, None)
                    .map_err(|e| CursorStoreError::Other(Box::new(e) as Box<_>))
            }
            TopicKind::GroupMessagesV1 => {
                let ids = vec![EntityKind::ApplicationMessage, EntityKind::CommitMessage];
                self.db
                    .latest_cursor_for_id(topic.identifier(), &ids, None)
                    .map_err(|e| CursorStoreError::Other(Box::new(e) as Box<_>))
            }
            TopicKind::IdentityUpdatesV1 => {
                let sid = self
                    .db
                    .get_latest_sequence_id_for_inbox(&hex::encode(topic.identifier()))
                    .map_err(|e| CursorStoreError::Other(Box::new(e) as Box<_>))?;
                let mut map = HashMap::new();
                map.insert(Originators::INBOX_LOG, sid as u64);
                Ok(GlobalCursor::new(map))
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
                    .map_err(|e| CursorStoreError::Other(Box::new(e) as Box<_>))
            }
            TopicKind::GroupMessagesV1 => {
                let entities = vec![EntityKind::ApplicationMessage, EntityKind::CommitMessage];
                self.db
                    .latest_cursor_for_id(topic.identifier(), &entities, Some(originators))
                    .map_err(|e| CursorStoreError::Other(Box::new(e) as Box<_>))
            }
            TopicKind::IdentityUpdatesV1 => {
                let sid = self
                    .db
                    .get_latest_sequence_id_for_inbox(&hex::encode(topic.identifier()))
                    .map_err(|e| CursorStoreError::Other(Box::new(e) as Box<_>))?;
                let mut map = HashMap::new();
                map.insert(Originators::INBOX_LOG, sid as u64);
                Ok(GlobalCursor::new(map))
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
                        .map_err(|e| CursorStoreError::Other(Box::new(e) as Box<_>))?;

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
                        .map_err(|e| CursorStoreError::Other(Box::new(e) as Box<_>))?;

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
                            .map_err(|e| CursorStoreError::Other(Box::new(e) as Box<_>))?;
                        let mut map = HashMap::new();
                        map.insert(Originators::INBOX_LOG, sid as u64);
                        Ok((topic.clone(), GlobalCursor::new(map)))
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
}
