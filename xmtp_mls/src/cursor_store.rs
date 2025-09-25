use xmtp_api_d14n::protocol::{CursorStore, CursorStoreError};
use xmtp_db::{prelude::QueryRefreshState, refresh_state::EntityKind};
use xmtp_proto::types::{Cursor, OriginatorId, Topic, TopicKind};

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
    Db: QueryRefreshState + Send + Sync,
{
    fn lowest_common_cursor(
        &self,
        topics: &[&Topic],
    ) -> Result<xmtp_proto::types::GlobalCursor, CursorStoreError> {
        self.db
            .lowest_common_cursor(topics)
            .map_err(|e| CursorStoreError::Other(Box::new(e) as Box<_>))
    }

    fn latest_for_each(
        &self,
        originators: &[OriginatorId],
        topic: &Topic,
    ) -> Result<Vec<Cursor>, CursorStoreError> {
        let entity = match topic.kind() {
            TopicKind::WelcomeMessagesV1 => EntityKind::Welcome,
            TopicKind::GroupMessagesV1 => EntityKind::Group,
            _ => unimplemented!(),
        };
        self.db
            .get_last_cursor_for_originators(topic.identifier(), entity, &originators)
            .map_err(|e| CursorStoreError::Other(Box::new(e) as Box<_>))
    }
}
