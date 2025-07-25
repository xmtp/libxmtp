use crate::groups::intents::GROUP_KEY_ROTATION_INTERVAL_NS;
use crate::groups::{GroupError, MlsGroup, XmtpSharedContext};
use crate::track;
use derive_builder::Builder;
use xmtp_db::{
    group_intent::{IntentKind, NewGroupIntent, StoredGroupIntent},
    prelude::*,
    ConnectionExt, DbQuery,
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
    fn queue_with_conn<Ctx, C>(
        &mut self,
        conn: &impl DbQuery<C>,
        group: &MlsGroup<Ctx>,
    ) -> Result<StoredGroupIntent, GroupError>
    where
        C: ConnectionExt,
        Ctx: XmtpSharedContext,
    {
        let intent = self.build()?;
        intent.queue_with_conn(conn, group)
    }

    /// Create an intent to send a message
    pub fn send_message(&mut self) -> &mut Self {
        self.kind = Some(IntentKind::SendMessage);
        self
    }

    /// Create an intent to update the keys for a group
    pub fn key_update(&mut self) -> &mut Self {
        self.kind = Some(IntentKind::KeyUpdate);
        self
    }

    /// Create an intent to update the metadata of a group
    pub fn metadata_update(&mut self) -> &mut Self {
        self.kind = Some(IntentKind::MetadataUpdate);
        self
    }

    /// Create an intent to update the membership of a group
    pub fn update_group_membership(&mut self) -> &mut Self {
        self.kind = Some(IntentKind::UpdateGroupMembership);
        self
    }

    /// Create an intent to update the admin list of a group
    pub fn update_admin_list(&mut self) -> &mut Self {
        self.kind = Some(IntentKind::UpdateAdminList);
        self
    }

    /// create an intent to update the permissions of a group
    pub fn update_permission(&mut self) -> &mut Self {
        self.kind = Some(IntentKind::UpdatePermission);
        self
    }
}

impl QueueIntent {
    pub fn builder() -> QueueIntentBuilder {
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

    fn queue_with_conn<Ctx, C>(
        self,
        conn: &impl DbQuery<C>,
        group: &MlsGroup<Ctx>,
    ) -> Result<StoredGroupIntent, GroupError>
    where
        Ctx: XmtpSharedContext,
        C: ConnectionExt,
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
    fn maybe_insert_key_update_intent<Ctx, C>(
        &self,
        conn: &impl DbQuery<C>,
        group: &MlsGroup<Ctx>,
    ) -> Result<(), GroupError>
    where
        Ctx: XmtpSharedContext,
        C: ConnectionExt,
    {
        let last_rotated_at_ns = conn.get_rotated_at_ns(group.group_id.clone())?;
        let now_ns = xmtp_common::time::now_ns();
        let elapsed_ns = now_ns - last_rotated_at_ns;
        if elapsed_ns > GROUP_KEY_ROTATION_INTERVAL_NS {
            QueueIntent::builder()
                .key_update()
                .queue_with_conn(conn, group)?;
        }
        Ok(())
    }
}
