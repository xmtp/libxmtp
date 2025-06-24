use crate::client::ClientError;
use crate::context::{XmtpContextProvider, XmtpMlsLocalContext};
use crate::intents::ProcessIntentError;
use crate::mls_store::MlsStore;
use crate::{
    groups::{GroupError, MlsGroup},
    XmtpApi,
};
use futures::stream::{self, FuturesUnordered, StreamExt};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use xmtp_common::{retry_async, Retry};
use xmtp_db::local_commit_log::NewLocalCommitLog;
use xmtp_db::remote_commit_log::CommitResult;
use xmtp_db::{consent_record::ConsentState, group::GroupQueryArgs};
use xmtp_db::{Store, XmtpDb};
use xmtp_proto::xmtp::mls::api::v1::{welcome_message, WelcomeMessage};

#[derive(Clone)]
pub struct WelcomeService<Api, Db> {
    context: Arc<XmtpMlsLocalContext<Api, Db>>,
}

impl<Api, Db> WelcomeService<Api, Db> {
    pub fn new(context: Arc<XmtpMlsLocalContext<Api, Db>>) -> Self {
        Self { context }
    }
}

impl<Api, Db> WelcomeService<Api, Db>
where
    Api: XmtpApi,
    Db: XmtpDb,
{
    /// Internal API to process a unread welcome message and convert to a group.
    /// In a database transaction, increments the cursor for a given installation and
    /// applies the update after the welcome processed successfully.
    async fn process_new_welcome(
        &self,
        welcome: &welcome_message::V1,
    ) -> Result<MlsGroup<Api, Db>, GroupError> {
        let result = MlsGroup::create_from_welcome(self.context.clone(), welcome).await;

        match result {
            Ok(mls_group) => Ok(mls_group),
            Err(err) => {
                use crate::DuplicateItem::*;
                use crate::StorageError::*;
                match &err {
                    GroupError::ProcessIntent(ProcessIntentError::WelcomeAlreadyProcessed(
                        _,
                        _,
                        group_id,
                    )) => {
                        let _ = NewLocalCommitLog {
                            group_id: group_id.clone(),
                            commit_sequence_id: welcome.id as i64,
                            last_epoch_authenticator: vec![],
                            commit_result: CommitResult::Invalid,
                            error_message: Some(err.to_string()),
                            applied_epoch_number: None,
                            applied_epoch_authenticator: None,
                            sender_inbox_id: None,
                            sender_installation_id: None,
                            commit_type: Some("Welcome Rejected".into()),
                        }
                        .store(self.context.mls_provider().db());
                    }
                    _ => {
                        let _ = NewLocalCommitLog {
                            group_id: vec![1, 1, 1, 2, 2, 2],
                            commit_sequence_id: welcome.id as i64,
                            last_epoch_authenticator: vec![],
                            commit_result: CommitResult::Invalid,
                            error_message: Some(err.to_string()),
                            applied_epoch_number: None,
                            applied_epoch_authenticator: None,
                            sender_inbox_id: None,
                            sender_installation_id: None,
                            commit_type: Some("Welcome Rejected".into()),
                        }
                        .store(self.context.mls_provider().db());
                    }
                }
                if matches!(err, GroupError::Storage(Duplicate(WelcomeId(_)))) {
                    tracing::error!(
                        "### welcome failed to create group from welcome due to duplicate welcome ID: {}",
                        err
                    );
                } else {
                    tracing::error!("### welcome failed to create group from welcome: {}", err);
                }

                Err(err)
            }
        }
    }
    /// Download all unread welcome messages and converts to a group struct, ignoring malformed messages.
    /// Returns any new groups created in the operation
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn sync_welcomes(&self) -> Result<Vec<MlsGroup<Api, Db>>, GroupError> {
        let provider = self.context.mls_provider();
        let store = MlsStore::new(self.context.clone());
        let envelopes = store.query_welcome_messages(provider.db()).await?;
        let num_envelopes = envelopes.len();

        // TODO: Update cursor correctly if some of the welcomes fail and some of the welcomes succeed
        let groups: Vec<MlsGroup<Api, Db>> = stream::iter(envelopes.into_iter())
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
                    (async { self.process_new_welcome(&welcome_v1).await })
                )
                .ok()
            })
            .collect()
            .await;

        // Rotate the keys regardless of whether the welcomes failed or succeeded. It is better to over-rotate than
        // to under-rotate, as the latter risks leaving expired key packages on the network. We already have a max
        // rotation interval.
        if num_envelopes > 0 {
            let provider = self.context.mls_provider();
            self.context.identity.queue_key_rotation(&provider).await?;
        }

        Ok(groups)
    }

    /// Sync all groups for the current installation and return the number of groups that were synced.
    /// Only active groups will be synced.
    pub async fn sync_all_groups(
        &self,
        groups: Vec<MlsGroup<Api, Db>>,
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
                        group.maybe_update_installations(None).await?;

                        group.sync_with_conn().await?;
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

    /// Sync all unread welcome messages and then sync all groups.
    /// Returns the total number of active groups synced.
    pub async fn sync_all_welcomes_and_groups(
        &self,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<usize, GroupError> {
        let provider = self.context.mls_provider();
        self.sync_welcomes().await?;
        let query_args = GroupQueryArgs {
            consent_states,
            include_duplicate_dms: true,
            include_sync_groups: true,
            ..GroupQueryArgs::default()
        };
        let groups = provider
            .db()
            .find_groups(query_args)?
            .into_iter()
            .map(|g| MlsGroup::new(self.context.clone(), g.id, g.dm_id, g.created_at_ns))
            .collect();
        let active_groups_count = self.sync_all_groups(groups).await?;

        Ok(active_groups_count)
    }

    pub async fn sync_all_welcomes_and_history_sync_groups(&self) -> Result<usize, ClientError> {
        let provider = self.context.mls_provider();
        self.sync_welcomes().await?;
        let groups = provider
            .db()
            .all_sync_groups()?
            .into_iter()
            .map(|g| MlsGroup::new(self.context.clone(), g.id, g.dm_id, g.created_at_ns))
            .collect();
        let active_groups_count = self.sync_all_groups(groups).await?;

        Ok(active_groups_count)
    }
}
