use crate::groups::intents::GROUP_KEY_ROTATION_INTERVAL_NS;
use crate::groups::{GroupError, MlsGroup, XmtpSharedContext};
use crate::track;
use derive_builder::Builder;
use xmtp_db::{
    group_intent::{IntentKind, NewGroupIntent, StoredGroupIntent},
    prelude::*,
    DbQuery,
};

#[derive(Builder, Debug)]
#[builder(setter(strip_option), build_fn(error = "GroupError", private))]
pub struct QueueIntent {
    #[builder(setter(into))]
    kind: IntentKind,
    /// not specifying data will be empty vec
    #[builder(setter(into), default)]
    data: Vec<u8>,
    #[builder(setter(into), default)]
    should_push: bool,
}

impl QueueIntentBuilder {
    pub fn queue<C>(&mut self, group: &MlsGroup<C>) -> Result<StoredGroupIntent, GroupError>
    where
        C: XmtpSharedContext,
    {
        let intent = self.build()?;
        intent.queue(group)
    }

    /// private api to queue an intent w/o starting a transaction
    fn queue_with_conn<Ctx>(
        &mut self,
        conn: &impl DbQuery,
        group: &MlsGroup<Ctx>,
    ) -> Result<StoredGroupIntent, GroupError>
    where
        Ctx: XmtpSharedContext,
    {
        let intent = self.build()?;
        intent.queue_with_conn(conn, group)
    }
}

impl QueueIntent {
    /// Create an intent to send a message
    pub fn send_message() -> QueueIntentBuilder {
        let mut this = QueueIntent::builder();
        this.kind = Some(IntentKind::SendMessage);
        this
    }

    /// Create an intent to update the keys for a group
    pub fn key_update() -> QueueIntentBuilder {
        let mut this = QueueIntent::builder();
        this.kind = Some(IntentKind::KeyUpdate);
        this
    }

    /// Create an intent to update the metadata of a group
    pub fn metadata_update() -> QueueIntentBuilder {
        let mut this = QueueIntent::builder();
        this.kind = Some(IntentKind::MetadataUpdate);
        this
    }

    /// Create an intent to update the membership of a group
    pub fn update_group_membership() -> QueueIntentBuilder {
        let mut this = QueueIntent::builder();
        this.kind = Some(IntentKind::UpdateGroupMembership);
        this
    }

    /// Create an intent to update the admin list of a group
    pub fn update_admin_list() -> QueueIntentBuilder {
        let mut this = QueueIntent::builder();
        this.kind = Some(IntentKind::UpdateAdminList);
        this
    }

    /// create an intent to update the permissions of a group
    pub fn update_permission() -> QueueIntentBuilder {
        let mut this = QueueIntent::builder();
        this.kind = Some(IntentKind::UpdatePermission);
        this
    }

    fn builder() -> QueueIntentBuilder {
        QueueIntentBuilder::default()
    }

    fn queue<C>(self, group: &MlsGroup<C>) -> Result<StoredGroupIntent, GroupError>
    where
        C: XmtpSharedContext,
    {
        group.context.mls_storage().transaction(move |conn| {
            let storage = conn.key_store();
            let db = storage.db();
            self.queue_with_conn(&db, group)
        })
    }

    fn queue_with_conn<Ctx>(
        self,
        conn: &impl DbQuery,
        group: &MlsGroup<Ctx>,
    ) -> Result<StoredGroupIntent, GroupError>
    where
        Ctx: XmtpSharedContext,
    {
        if self.kind == IntentKind::SendMessage {
            self.maybe_insert_key_update_intent(conn, group)?;
        }

        let Self {
            kind: intent_kind,
            data: intent_data,
            should_push,
        } = self;

        let intent = conn.insert_group_intent(NewGroupIntent::new(
            intent_kind,
            group.group_id.clone(),
            intent_data,
            should_push,
        ))?;

        if intent_kind != IntentKind::SendMessage {
            conn.update_rotated_at_ns(group.group_id.clone())?;

            track!(
                "Queue Intent",
                { "intent_kind": intent_kind },
                group: &group.group_id
            );
        }
        tracing::debug!(inbox_id = group.context.inbox_id(), intent_kind = %intent_kind, "queued intent");

        Ok(intent)
    }

    #[tracing::instrument(level = "trace", skip_all)]
    fn maybe_insert_key_update_intent<Ctx>(
        &self,
        conn: &impl DbQuery,
        group: &MlsGroup<Ctx>,
    ) -> Result<(), GroupError>
    where
        Ctx: XmtpSharedContext,
    {
        let last_rotated_at_ns = conn.get_rotated_at_ns(group.group_id.clone())?;
        let now_ns = xmtp_common::time::now_ns();
        let elapsed_ns = now_ns - last_rotated_at_ns;
        if elapsed_ns > GROUP_KEY_ROTATION_INTERVAL_NS {
            QueueIntent::key_update().queue_with_conn(conn, group)?;
        }
        Ok(())
    }
}
