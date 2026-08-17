use crate::groups::intents::GROUP_KEY_ROTATION_INTERVAL_NS;
use crate::groups::{GroupError, MlsGroup, XmtpSharedContext};
use derive_builder::Builder;
use xmtp_db::TransactionOutcome::Continue;
use xmtp_db::{
    DbQuery, TransactionOutcome,
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
        group
            .context
            .mls_storage()
            .transaction(async move |conn| {
                let storage = conn.key_store();
                let db = storage.db();
                intent.queue_with_conn(&db, group).await.map(Continue)
            })
            .map(TransactionOutcome::into_continued)
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

    /// Create an intent to fire the one-shot AppData-migration
    /// bootstrap commit. The intent payload is a
    /// [`ProposeGroupContextExtensionsIntentData`] carrying the
    /// target extensions blob (four legacy XMTP extensions removed,
    /// `RequiredCapabilities` updated to require
    /// `ExtensionType::AppDataDictionary` and drop the legacy
    /// extension types). The handler synthesizes the per-component
    /// dict seeds and bundles everything into a single commit.
    pub fn bootstrap_migration() -> QueueIntentBuilder {
        let mut this = QueueIntent::builder();
        this.kind = Some(IntentKind::BootstrapMigration);
        this
    }

    /// Create an intent to write to an AppData well-known or app-range
    /// component. The intent's `data` field carries a serialized
    /// [`crate::groups::intents::AppDataUpdateIntentData`] — see that
    /// type's documentation for the merge semantics of each op flavor.
    pub fn app_data_update() -> QueueIntentBuilder {
        let mut this = QueueIntent::builder();
        this.kind = Some(IntentKind::AppDataUpdate);
        this
    }

    fn builder() -> QueueIntentBuilder {
        QueueIntentBuilder::default()
    }

    async fn queue_with_conn<Ctx>(
        self,
        conn: &impl DbQuery,
        group: &MlsGroup<Ctx>,
    ) -> Result<StoredGroupIntent, GroupError>
    where
        Ctx: XmtpSharedContext,
    {
        if self.kind == IntentKind::SendMessage {
            self.maybe_insert_key_update_intent(conn, group).await?;
        }

        let Self {
            kind: intent_kind,
            data: intent_data,
            should_push,
        } = self;

        let intent = conn
            .insert_group_intent(NewGroupIntent::new(
                intent_kind,
                group.group_id,
                intent_data,
                should_push,
            ))
            .await?;

        if intent_kind != IntentKind::SendMessage {
            conn.update_rotated_at_ns(&group.group_id).await?;
        }
        tracing::debug!(inbox_id = group.context.inbox_id(), intent_kind = %intent_kind, "queued intent");

        Ok(intent)
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn maybe_insert_key_update_intent<Ctx>(
        &self,
        conn: &impl DbQuery,
        group: &MlsGroup<Ctx>,
    ) -> Result<(), GroupError>
    where
        Ctx: XmtpSharedContext,
    {
        let last_rotated_at_ns = conn.get_rotated_at_ns(&group.group_id).await?;
        let now_ns = xmtp_common::time::now_ns();
        let elapsed_ns = now_ns - last_rotated_at_ns;
        if elapsed_ns > GROUP_KEY_ROTATION_INTERVAL_NS {
            // Boxed to break the `queue_with_conn` -> here -> `queue_with_conn`
            // cycle; a recursive async fn has no finite size without indirection.
            // The recursion is only ever one level deep: this queues a `key_update`,
            // and only `SendMessage` re-enters this function.
            Box::pin(
                QueueIntent::key_update()
                    .build()?
                    .queue_with_conn(conn, group),
            )
            .await?;
        }
        Ok(())
    }
}
