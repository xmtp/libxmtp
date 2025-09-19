use xmtp_api_d14n::protocol::CursorStore;
use xmtp_db::{DbQuery, StorageError, prelude::QueryRefreshState, refresh_state::EntityKind};
use xmtp_proto::types::{Cursor, OriginatorId, Topic, TopicKind};

pub struct V3CursorStore<Db> {
    db: Db,
}

impl<Db> CursorStore for V3CursorStore<Db>
where
    Db: QueryRefreshState,
{
    type Error = StorageError;

    fn lowest_common_cursor(
        &self,
        topics: &[Topic],
    ) -> Result<xmtp_proto::types::GlobalCursor, Self::Error> {
        self.db.lowest_common_cursor(topics)
    }

    fn latest(&self, originators: Vec<OriginatorId>, topic: &Topic) -> Vec<Cursor> {
        let entity = match topic.kind {
            TopicKind::WelcomeMessagesV1 => EntityKind::Welcome,
            TopicKind::GroupMessagesV1 => EntityKind::Group,
            _ => unimplemented!(),
        };
        self.db
            .get_last_cursor_for_originators(topic.identifier(), entity, originators)
    }
}

pub struct D14nCursorStore<Db> {
    db: Db,
}
