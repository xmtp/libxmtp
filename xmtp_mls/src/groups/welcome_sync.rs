use crate::client::ClientError;
use crate::context::XmtpSharedContext;
use crate::groups::{GroupError, MlsGroup};
use crate::mls_store::MlsStore;
use futures::stream::{self, FuturesUnordered, StreamExt};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tracing::{trace_span, Instrument};
use xmtp_common::{retry_async, Retry};
use xmtp_db::{consent_record::ConsentState, group::GroupQueryArgs, prelude::*};
use xmtp_proto::xmtp::mls::api::v1::{welcome_message, WelcomeMessage};

#[derive(Clone)]
pub struct WelcomeService<Context> {
    context: Context,
}

impl<Context> WelcomeService<Context> {
    pub fn new(context: Context) -> Self {
        Self { context }
    }
}

impl<Context> WelcomeService<Context>
where
    Context: XmtpSharedContext,
{
    /// Internal API to process a unread welcome message and convert to a group.
    /// In a database transaction, increments the cursor for a given installation and
    /// applies the update after the welcome processed successfully.
    pub(crate) async fn process_new_welcome(
        &self,
        welcome: &welcome_message::V1,
        cursor_increment: bool,
    ) -> Result<MlsGroup<Context>, GroupError> {
        let result =
            MlsGroup::create_from_welcome(self.context.clone(), welcome, cursor_increment).await;

        match result {
            Ok(mls_group) => Ok(mls_group),
            Err(err) => {
                use crate::DuplicateItem::*;
                use crate::StorageError::*;

                if matches!(err, GroupError::Storage(Duplicate(WelcomeId(_)))) {
                    tracing::warn!(
                        "failed to create group from welcome due to duplicate welcome ID: {}",
                        err
                    );
                } else {
                    tracing::error!(
                        "failed to create group from welcome created at {}: {}",
                        welcome.created_ns,
                        err
                    );
                }

                Err(err)
            }
        }
    }

    /// Download all unread welcome messages and converts to a group struct, ignoring malformed messages.
    /// Returns any new groups created in the operation
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn sync_welcomes(&self) -> Result<Vec<MlsGroup<Context>>, GroupError> {
        let db = self.context.db();
        let store = MlsStore::new(self.context.clone());
        let envelopes = store.query_welcome_messages(&db).await?;
        let num_envelopes = envelopes.len();

        // TODO: Update cursor correctly if some of the welcomes fail and some of the welcomes succeed
        let groups: Vec<MlsGroup<Context>> = stream::iter(envelopes.into_iter())
            .filter_map(|envelope: WelcomeMessage| async {
                let welcome_v1 = match envelope.version {
                    Some(welcome_message::Version::V1(v1)) => v1,
                    _ => {
                        tracing::error!(
                            "failed to extract welcome message, invalid payload only v1 supported."
                        );
                        return None;
                    }
                };
                retry_async!(
                    Retry::default(),
                    (async { self.process_new_welcome(&welcome_v1, true).await })
                )
                .ok()
            })
            .collect()
            .await;

        // Rotate the keys regardless of whether the welcomes failed or succeeded. It is better to over-rotate than
        // to under-rotate, as the latter risks leaving expired key packages on the network. We already have a max
        // rotation interval.
        if num_envelopes > 0 {
            self.context.identity().queue_key_rotation(&db).await?;
        }

        Ok(groups)
    }

    /// Sync all groups for the current installation and return the number of groups that were synced.
    /// Only active groups will be synced.
    #[tracing::instrument(skip_all, level = "trace")]
    pub async fn sync_all_groups(
        &self,
        groups: Vec<MlsGroup<Context>>,
    ) -> Result<usize, GroupError> {
        let active_group_count = Arc::new(AtomicUsize::new(0));

        let sync_futures = groups
            .into_iter()
            .map(|group| {
                let active_group_count = Arc::clone(&active_group_count);
                async move {
                    tracing::info!(
                        inbox_id = self.context.inbox_id(),
                        "[{}] syncing group",
                        self.context.inbox_id()
                    );
                    let is_active = group
                        .load_mls_group_with_lock_async(|mls_group| async move {
                            Ok::<bool, GroupError>(mls_group.is_active())
                        })
                        .await?;
                    if is_active {
                        group.sync_with_conn().await?;
                        group.maybe_update_installations(None).await?;
                        active_group_count.fetch_add(1, Ordering::SeqCst);
                    }

                    Ok::<(), GroupError>(())
                }
            })
            .collect::<FuturesUnordered<_>>();

        sync_futures
            .collect::<Vec<Result<_, _>>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;

        Ok(active_group_count.load(Ordering::SeqCst))
    }

    #[tracing::instrument(skip_all, level = "trace")]
    pub async fn sync_all_welcomes_and_history_sync_groups(&self) -> Result<usize, ClientError> {
        let db = self.context.db();
        self.sync_welcomes().await?;
        let groups = db
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
        let active_groups_count = self.sync_all_groups(groups).await?;

        Ok(active_groups_count)
    }

    /// Sync all unread welcome messages and then sync groups in descending order of recent activity.
    /// Returns number of active groups successfully synced.
    #[tracing::instrument(skip_all, level = "trace")]
    pub async fn sync_all_welcomes_and_groups(
        &self,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<usize, GroupError> {
        let db = self.context.db();

        if let Err(err) = self.sync_welcomes().await {
            tracing::warn!(?err, "sync_welcomes failed, continuing with group sync");
        }

        let query_args = GroupQueryArgs {
            consent_states,
            include_duplicate_dms: true,
            include_sync_groups: true,
            ..GroupQueryArgs::default()
        };

        let conversations = db.fetch_conversation_list(query_args)?;

        let groups: Vec<MlsGroup<Context>> = conversations
            .into_iter()
            .map(|c| {
                MlsGroup::new(
                    self.context.clone(),
                    c.id,
                    c.dm_id,
                    c.conversation_type,
                    c.created_at_ns,
                )
            })
            .collect();

        let success_count = self.sync_groups_in_batches(groups, 10).await?;

        Ok(success_count)
    }

    /// Sync groups concurrently with a limit. Returns success count.
    #[tracing::instrument(skip_all, level = "trace")]
    pub async fn sync_groups_in_batches(
        &self,
        groups: Vec<MlsGroup<Context>>,
        max_concurrency: usize,
    ) -> Result<usize, GroupError> {
        let active_group_count = Arc::new(AtomicUsize::new(0));
        let failed_group_count = Arc::new(AtomicUsize::new(0));

        stream::iter(groups)
            .for_each_concurrent(max_concurrency, |group| {
                let group = group.clone();
                let active_group_count = Arc::clone(&active_group_count);
                let failed_group_count = Arc::clone(&failed_group_count);
                let inbox_id = self.context.inbox_id();
                let span = trace_span!("concurrent_group_sync");
                async move {
                    tracing::info!(inbox_id, "[{}] syncing group", inbox_id);

                    let is_active_res = group
                        .load_mls_group_with_lock_async(|mls_group| async move {
                            Ok::<bool, GroupError>(mls_group.is_active())
                        })
                        .await;

                    match is_active_res {
                        Ok(is_active) if is_active => {
                            if let Err(err) = group.sync_with_conn().await {
                                tracing::warn!(?err, "sync_with_conn failed");
                                failed_group_count.fetch_add(1, Ordering::SeqCst);
                                return;
                            }

                            if let Err(err) = group.maybe_update_installations(None).await {
                                tracing::warn!(?err, "maybe_update_installations failed");
                                failed_group_count.fetch_add(1, Ordering::SeqCst);
                                return;
                            }

                            active_group_count.fetch_add(1, Ordering::SeqCst);
                        }
                        Ok(_) => { /* group inactive, skip */ }
                        Err(err) => {
                            tracing::warn!(?err, "load_mls_group_with_lock_async failed");
                            failed_group_count.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                }
                .instrument(span)
            })
            .await;

        Ok(active_group_count.load(Ordering::SeqCst))
    }
}
