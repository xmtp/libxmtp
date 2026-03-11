use futures::{StreamExt, TryFutureExt, stream};
use std::collections::HashSet;
use std::future::Future;

use crate::groups::intents::GROUP_KEY_ROTATION_INTERVAL_NS;
use crate::groups::{GroupError, MlsGroup, XmtpSharedContext};
use derive_builder::Builder;
use itertools::Itertools;
use xmtp_db::{
    DbQuery,
    group_intent::{IntentKind, NewGroupIntent, StoredGroupIntent},
    prelude::*,
};

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
    pub async fn queue_for_each<'a, C, F, Fut, E>(
        &mut self,
        groups: HashSet<MlsGroup<C>>,
        data: F,
    ) -> Result<Vec<StoredGroupIntent>, GroupError>
    where
        C: XmtpSharedContext + 'a,
        F: Fn(MlsGroup<C>) -> Fut,
        Fut: Future<Output = Result<Vec<u8>, E>>,
        GroupError: From<E>,
        E: std::fmt::Debug + std::error::Error,
    {
        if groups.is_empty() {
            return Ok(vec![]);
        }

        let groups: Vec<MlsGroup<C>> = Vec::from_iter(groups);
        let context: C = groups.first().expect("checked for empty").context.clone();

        // get the intent data for each group
        let (groups, errors): (Vec<_>, Vec<_>) = stream::iter(groups)
            .map(|group| data(group.clone()).map_ok(move |d| (group, d)))
            .buffered(10)
            .collect::<Vec<Result<_, _>>>()
            .await
            .into_iter()
            .partition_result();
        let mut errors = errors.into_iter().map(GroupError::from).collect::<Vec<_>>();

        let (intents, errs): (Vec<StoredGroupIntent>, Vec<_>) = {
            context.mls_storage().transaction(|conn| {
                let intents = groups
                    .into_iter()
                    .map(|(group, data)| {
                        let storage = conn.key_store();

                        // nesting a transaction uses SQLite Savepoints
                        // https://sqlite.org/lang_savepoint.html
                        storage.savepoint(|conn| {
                            let intent = self.clone().data(data).build()?;
                            let storage = conn.key_store();
                            let db = storage.db();
                            intent.queue_with_conn(&db, &group)
                        })
                    })
                    .partition_result();
                Ok::<_, GroupError>(intents)
            })?
        };
        errors.extend(errs);

        for error in errors {
            tracing::warn!("failed to queue intent {error}");
        }
        Ok(intents)
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

    pub fn readd_installations() -> QueueIntentBuilder {
        let mut this = QueueIntent::builder();
        this.kind = Some(IntentKind::ReaddInstallations);
        this
    }

    /// Create an intent to propose member updates (adds and/or removes)
    pub fn propose_member_update() -> QueueIntentBuilder {
        let mut this = QueueIntent::builder();
        this.kind = Some(IntentKind::ProposeMemberUpdate);
        this
    }

    /// Create an intent to propose group context extensions update
    pub fn propose_group_context_extensions() -> QueueIntentBuilder {
        let mut this = QueueIntent::builder();
        this.kind = Some(IntentKind::ProposeGroupContextExtensions);
        this
    }

    /// Create an intent to commit pending proposals
    pub fn commit_pending_proposals() -> QueueIntentBuilder {
        let mut this = QueueIntent::builder();
        this.kind = Some(IntentKind::CommitPendingProposals);
        this
    }

    fn builder() -> QueueIntentBuilder {
        QueueIntentBuilder::default()
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
            QueueIntent::key_update()
                .build()?
                .queue_with_conn(conn, group)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{iter, sync::Arc};
    use tokio::sync::Mutex;
    use xmtp_db::group::{GroupMembershipState, StoredGroup};

    use crate::test::mock::{NewMockContext, context};

    use super::*;
    use rstest::*;

    fn group<C: XmtpSharedContext>(context: &C, id: Option<Vec<u8>>) -> MlsGroup<&C> {
        let id = id.unwrap_or(xmtp_common::rand_vec::<2>());
        StoredGroup::builder()
            .id(id.clone())
            .created_at_ns(1)
            .membership_state(GroupMembershipState::Allowed)
            .added_by_inbox_id("bob")
            .should_publish_commit_log(false)
            .build()
            .unwrap()
            .store(&context.mls_storage().db())
            .unwrap();

        MlsGroup {
            group_id: id,
            dm_id: None,
            created_at_ns: 1,
            context,
            mls_commit_lock: Arc::new(Default::default()),
            mutex: Arc::new(Mutex::new(())),
            conversation_type: xmtp_db::group::ConversationType::Dm,
        }
    }

    #[rstest]
    #[xmtp_common::test]
    async fn can_queue_intent_for_each_group(context: NewMockContext) {
        let groups = iter::repeat_with(|| group(&context, None)).take(10);
        let intents = QueueIntent::update_group_membership()
            .data(vec![0, 1, 2])
            .queue_for_each(groups.collect(), async |_| Ok::<_, GroupError>(vec![]))
            .await
            .unwrap();
        assert_eq!(intents.len(), 10);
    }

    #[rstest]
    #[xmtp_common::test]
    async fn only_queues_for_unique(context: NewMockContext) {
        let groups = iter::repeat_n(group(&context, Some(vec![0])), 10);

        let stored = QueueIntent::update_group_membership()
            .data(vec![0, 1, 2])
            .queue_for_each(groups.collect(), async |_g| Ok::<_, GroupError>(vec![]))
            .await
            .unwrap();

        assert_eq!(stored.len(), 1);
    }
}
