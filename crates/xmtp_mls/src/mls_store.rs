//! Higher level queries against the local database
//! These queries return their mls-typed equivalents after converting
//! from the data in DB/Api
use std::collections::HashMap;

use xmtp_api::ApiError;
use xmtp_common::RetryableError;
use xmtp_db::{
    Fetch, NotFound, XmtpOpenMlsProvider,
    group::{GroupQueryArgs, StoredGroup},
};
use xmtp_proto::types::{GroupId, GroupMessage, InstallationId, Topic, WelcomeMessage};

use crate::{context::XmtpSharedContext, groups::MlsGroup};
use xmtp_id::key_package::{KeyPackageVerificationError, VerifiedKeyPackageV2};

use thiserror::Error;
use xmtp_db::prelude::*;

#[derive(Error, Debug)]
pub enum MlsStoreError {
    #[error(transparent)]
    Storage(#[from] xmtp_db::StorageError),
    #[error(transparent)]
    Api(#[from] ApiError),
    #[error(transparent)]
    Connection(#[from] xmtp_db::ConnectionError),
    #[error(transparent)]
    NotFound(#[from] NotFound),
}

impl RetryableError for MlsStoreError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Storage(e) => e.is_retryable(),
            Self::Api(e) => e.is_retryable(),
            Self::Connection(e) => e.is_retryable(),
            Self::NotFound(e) => e.is_retryable(),
        }
    }
}

impl crate::worker::NeedsDbReconnect for MlsStoreError {
    /// Forwards a dropped-pool signal from the storage/connection variants so a
    /// worker loading groups can stop on disconnect. `Api`/`NotFound` return `false`.
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(s) => s.db_needs_connection(),
            Self::Connection(c) => c.db_needs_connection(),
            Self::Api(_) | Self::NotFound(_) => false,
        }
    }
}

#[derive(Clone)]
pub struct MlsStore<Context> {
    context: Context,
}

impl<Context> MlsStore<Context> {
    pub fn new(context: Context) -> Self {
        Self { context }
    }
}

impl<Context> MlsStore<Context>
where
    Context: XmtpSharedContext,
{
    /// Query for welcome messages that have a `sequence_id` > than the highest cursor
    /// found in the local database
    pub(crate) async fn query_welcome_messages(
        &self,
    ) -> Result<Vec<WelcomeMessage>, MlsStoreError> {
        let installation_id = self.context.installation_id();

        let cursor = self
            .context
            .db()
            .get_last_cursor(installation_id, xmtp_db::refresh_state::EntityKind::Welcome)?;
        let welcomes = self
            .context
            .api()
            .query_welcome_messages_with_cursors(
                [(Topic::new_welcome_message(installation_id), cursor)].into(),
            )
            .await?;
        tracing::debug!("returning {} welcomes", welcomes.len());
        Ok(welcomes)
    }

    /// Query for group messages that have a `sequence_id` > than the highest cursor
    /// found in the local database
    pub(crate) async fn query_group_messages(
        &self,
        group_id: GroupId,
    ) -> Result<Vec<GroupMessage>, MlsStoreError> {
        use xmtp_db::refresh_state::EntityKind;
        let db = self.context.db();
        let application = db.get_last_cursor(group_id, EntityKind::ApplicationMessage)?;
        let commit = db.get_last_cursor(group_id, EntityKind::CommitMessage)?;
        let messages = self
            .context
            .api()
            .query_group_messages_with_cursors(
                [(Topic::new_group_message(group_id), application.min(commit))].into(),
            )
            .await?;
        // One topic contains both kinds. Discard each kind's stored prefix.
        let messages = messages
            .into_iter()
            .filter(|message| {
                message.cursor
                    > if message.is_commit() {
                        commit
                    } else {
                        application
                    }
            })
            .collect();

        Ok(messages)
    }

    /// Fetches the current key package from the network for each of the `installation_id`s specified
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn get_key_packages_for_installation_ids(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<
        HashMap<Vec<u8>, Result<VerifiedKeyPackageV2, KeyPackageVerificationError>>,
        MlsStoreError,
    > {
        let installation_ids = installation_ids
            .into_iter()
            .map(InstallationId::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ApiError::from)?;
        let key_package_results = self
            .context
            .api()
            .fetch_key_packages(&installation_ids)
            .await?;

        let crypto_provider = XmtpOpenMlsProvider::<()>::new_crypto();

        let results: HashMap<Vec<u8>, Result<VerifiedKeyPackageV2, KeyPackageVerificationError>> =
            key_package_results
                .iter()
                .filter_map(|(id, package)| {
                    let package = package.as_ref()?;
                    Some((
                        id.to_vec(),
                        VerifiedKeyPackageV2::from_bytes(
                            &crypto_provider,
                            &package.key_package_tls_serialized,
                        ),
                    ))
                })
                .collect();

        Ok(results)
    }

    /// Query for groups with optional filters
    ///
    /// Filters:
    /// - allowed_states: only return groups with the given membership states
    /// - created_after_ns: only return groups created after the given timestamp (in nanoseconds)
    /// - created_before_ns: only return groups created before the given timestamp (in nanoseconds)
    /// - limit: only return the first `limit` groups
    pub fn find_groups(
        &self,
        args: GroupQueryArgs,
    ) -> Result<Vec<MlsGroup<Context>>, MlsStoreError> {
        Ok(self
            .context
            .db()
            .find_groups(args)?
            .into_iter()
            .map(|stored_group| {
                MlsGroup::new(
                    self.context.clone(),
                    stored_group.id,
                    stored_group.dm_id,
                    stored_group.conversation_type,
                    stored_group.created_at_ns,
                )
            })
            .collect())
    }

    /// Look up a group by its ID
    ///
    /// Returns a [`MlsGroup`] if the group exists, or an error if it does not
    ///
    pub fn group(&self, group_id: &GroupId) -> Result<MlsGroup<Context>, MlsStoreError> {
        let conn = self.context.db();
        let stored_group: Option<StoredGroup> = conn.fetch(group_id)?;
        stored_group
            .map(|g| {
                MlsGroup::new(
                    self.context.clone(),
                    g.id,
                    g.dm_id,
                    g.conversation_type,
                    g.created_at_ns,
                )
            })
            .ok_or(NotFound::GroupById(*group_id))
            .map_err(Into::into)
    }
}
