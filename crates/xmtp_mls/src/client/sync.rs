//! Client sync: welcome, group and device-sync group sync.

use super::*;

impl<Context> Client<Context>
where
    Context: XmtpSharedContext,
{
    /// Download all unread welcome messages and converts to a group struct, ignoring malformed messages.
    /// Returns any new groups created in the operation
    #[tracing::instrument(skip_all)]
    pub async fn sync_welcomes(&self) -> Result<Vec<MlsGroup<Context>>, GroupError> {
        self.ensure_identity_ready::<GroupError>()?;
        WelcomeService::new(self.context.clone())
            .sync_welcomes()
            .await
            .map_err(|error| self.context.server_configuration().blocked_or(error))
    }

    /// Sync all groups for the current installation and return the number of groups that were synced.
    /// Only active groups will be synced.
    #[tracing::instrument(err, skip_all, fields(operation = "sync_all_groups"))]
    pub async fn sync_all_groups(
        &self,
        groups: Vec<MlsGroup<Context>>,
    ) -> Result<GroupSyncSummary, GroupError> {
        self.ensure_identity_ready::<GroupError>()?;
        WelcomeService::new(self.context.clone())
            .sync_all_groups(groups)
            .await
            .map_err(|error| self.context.server_configuration().blocked_or(error))
    }

    /// Sync all unread welcome messages and then sync all groups.
    /// Returns the total number of active groups synced.
    #[xmtp_common::mls_span]
    pub async fn sync_all_welcomes_and_groups(
        &self,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<GroupSyncSummary, GroupError> {
        self.ensure_identity_ready::<GroupError>()?;
        WelcomeService::new(self.context.clone())
            .sync_all_welcomes_and_groups(consent_states)
            .await
            .map_err(|error| self.context.server_configuration().blocked_or(error))
    }

    /// Sweep every group flagged `paused_for_version` and clear the
    /// pause flag for any whose floor is now satisfied by this
    /// client's `pkg_version`. Pure local-state operation — no
    /// network calls. Returns the count of groups unstuck.
    ///
    /// `sync_all_welcomes_and_groups` already runs this sweep as a
    /// preamble; the standalone entry point is for SDKs that want a
    /// cheap "post-upgrade recovery" hook independent of the normal
    /// sync flow.
    pub async fn unstick_paused_groups(&self) -> Result<usize, GroupError> {
        self.ensure_identity_ready::<GroupError>()?;
        WelcomeService::new(self.context.clone())
            .unstick_paused_groups()
            .await
    }

    pub async fn sync_all_welcomes_and_device_sync_groups(
        &self,
    ) -> Result<GroupSyncSummary, ClientError> {
        self.sync_welcomes().await?;
        self.sync_all_device_sync_groups().await
    }

    pub async fn sync_all_device_sync_groups(&self) -> Result<GroupSyncSummary, ClientError> {
        let groups = self
            .context
            .db()
            .all_sync_groups()?
            .into_iter()
            .map(|g| {
                MlsGroup::new(
                    self.context.clone(),
                    g.id,
                    g.dm_id,
                    g.conversation_type,
                    g.created_at_ns,
                )
            })
            .collect();

        Ok(self.sync_all_groups(groups).await?)
    }
}
