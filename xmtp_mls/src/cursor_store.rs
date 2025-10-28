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
        tracing::info!(
            "getting latest per originator for originators {:?}",
            originators
        );
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

    fn latest_maybe_missing_per(
        &self,
        topic: &Topic,
        originators: &[&OriginatorId],
    ) -> Result<GlobalCursor, CursorStoreError> {
        match topic.kind() {
            TopicKind::WelcomeMessagesV1 => {
                let entities = vec![EntityKind::Welcome];
                self.db
                    .latest_cursor_combined(topic.identifier(), &entities, Some(originators))
                    .map_err(|e| CursorStoreError::Other(Box::new(e) as Box<_>))
            }
            TopicKind::GroupMessagesV1 => {
                let entities = vec![EntityKind::ApplicationMessage, EntityKind::CommitMessage];
                self.db
                    .latest_cursor_combined(topic.identifier(), &entities, Some(originators))
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
}
