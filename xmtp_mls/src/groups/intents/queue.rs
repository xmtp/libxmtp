use futures::{stream, StreamExt, TryFutureExt, TryStreamExt};
use std::collections::HashSet;

use crate::groups::intents::GROUP_KEY_ROTATION_INTERVAL_NS;
use crate::groups::{GroupError, MlsGroup, XmtpSharedContext};
use crate::track;
use derive_builder::Builder;
use xmtp_db::{
    group_intent::{IntentKind, NewGroupIntent, StoredGroupIntent},
    prelude::*,
    ConnectionExt, DbQuery,
};

#[derive(Builder, Clone, Debug)]
pub struct BatchQueueIntents {}

#[derive(Builder, Clone, Debug)]
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
        group.context.mls_storage().transaction(move |conn| {
            let storage = conn.key_store();
            let db = storage.db();
            intent.queue_with_conn(&db, group)
        })
    }

    /// Queue this intent for each group
    /// Accepts a hashset to ensure each group is unique
    /// Accepts a closure that returns the intent data for a group
    pub async fn queue_for_each<C, F, E>(
        &mut self,
        groups: &HashSet<MlsGroup<C>>,
        data: F,
    ) -> Result<Vec<StoredGroupIntent>, GroupError>
    where
        C: XmtpSharedContext,
        F: AsyncFn(&MlsGroup<C>) -> Result<Vec<u8>, E>,
        GroupError: From<E>,
    {
        if groups.is_empty() {
            return Ok(vec![]);
        }

        // get the intent data for each group
        let groups: Vec<(&MlsGroup<C>, Vec<u8>)> = stream::iter(groups)
            .map(|group| data(&group).map_ok(move |d| (group, d)))
            .buffered(10)
            .try_collect()
            .await?;

        let intents: Vec<StoredGroupIntent> = {
            let first_group = groups
                .first()
                .expect("checked for existence of at least one group");
            let context = &first_group.0.context;

            context.mls_storage().transaction(move |conn| {
                let storage = conn.key_store();
                let db = storage.db();

                groups
                    .into_iter()
                    .map(|(group, data)| self.clone().data(data).queue_with_conn(&db, group))
                    .collect::<Result<Vec<_>, _>>()
            })?
        };
        Ok(intents)
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
                .queue_with_conn(conn, &group)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use xmtp_db::group::{GroupMembershipState, StoredGroup};

    use crate::test::mock::{context, NewMockContext};

    use super::*;
    use rstest::*;

    #[rstest]
    #[xmtp_common::test]
    async fn can_queue_intent_for_each_group(mut context: NewMockContext) {
        let db = context.mls_storage().db();

        StoredGroup::builder()
            .id(vec![0])
            .created_at_ns(1)
            .membership_state(GroupMembershipState::Allowed)
            .added_by_inbox_id("bob")
            .should_publish_commit_log(false)
            .build()
            .unwrap()
            .store(&db);

        let group = MlsGroup::<NewMockContext> {
            group_id: vec![0],
            dm_id: None,
            created_at_ns: 1,
            context: context,
            mls_commit_lock: Arc::new(Default::default()),
            mutex: Arc::new(Mutex::new(())),
        };

        QueueIntent::builder()
            .update_group_membership()
            .data(vec![0, 1, 2])
            .queue(&group)
            .unwrap();
    }
}
