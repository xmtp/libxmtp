//! Higher level queries against the local database
//! These queries return their mls-typed equivalents after converting
//! from the data in DB/Api
use std::{collections::HashMap, sync::Arc};

use xmtp_api::{ApiError, XmtpApi};
use xmtp_common::RetryableError;
use xmtp_db::{
    group::{GroupQueryArgs, StoredGroup},
    refresh_state::EntityKind,
    DbConnection, Fetch, NotFound, XmtpDb, XmtpOpenMlsProvider,
};
use xmtp_proto::mls_v1::{GroupMessage, WelcomeMessage};

use crate::{
    context::{XmtpContextProvider, XmtpMlsLocalContext},
    groups::MlsGroup,
    verified_key_package_v2::{KeyPackageVerificationError, VerifiedKeyPackageV2},
};
use thiserror::Error;

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

#[derive(Clone)]
pub struct MlsStore<ApiClient, Db> {
    context: Arc<XmtpMlsLocalContext<ApiClient, Db>>,
}

impl<ApiClient, Db> MlsStore<ApiClient, Db> {
    pub fn new(context: Arc<XmtpMlsLocalContext<ApiClient, Db>>) -> Self {
        Self { context }
    }
}

impl<ApiClient, Db> MlsStore<ApiClient, Db>
where
    ApiClient: XmtpApi,
    Db: XmtpDb,
{
    /// Query for welcome messages that have a `sequence_id` > than the highest cursor
    /// found in the local database
    pub(crate) async fn query_welcome_messages(
        &self,
        conn: &DbConnection<<Db as XmtpDb>::Connection>,
    ) -> Result<Vec<WelcomeMessage>, MlsStoreError> {
        let installation_id = self.context.installation_id();
        let id_cursor = conn.get_last_cursor_for_id(installation_id, EntityKind::Welcome)?;

        let welcomes = self
            .context
            .api()
            .query_welcome_messages(installation_id.as_ref(), Some(id_cursor as u64))
            .await?;

        Ok(welcomes)
    }

    /// Query for group messages that have a `sequence_id` > than the highest cursor
    /// found in the local database
    pub(crate) async fn query_group_messages(
        &self,
        group_id: &[u8],
        conn: &DbConnection<<Db as XmtpDb>::Connection>,
        limit: Option<u32>,
    ) -> Result<Vec<GroupMessage>, MlsStoreError> {
        let id_cursor = conn.get_last_cursor_for_id(group_id, EntityKind::Group)?;

        let messages = self
            .context
            .api()
            .query_group_messages(group_id.to_vec(), Some(id_cursor as u64), limit)
            .await?;

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
        let key_package_results = self
            .context
            .api()
            .fetch_key_packages(installation_ids.clone())
            .await?;

        let crypto_provider = XmtpOpenMlsProvider::new_crypto();

        let results: HashMap<Vec<u8>, Result<VerifiedKeyPackageV2, KeyPackageVerificationError>> =
            key_package_results
                .iter()
                .map(|(id, bytes)| {
                    (
                        id.clone(),
                        VerifiedKeyPackageV2::from_bytes(&crypto_provider, bytes),
                    )
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
    ) -> Result<Vec<MlsGroup<ApiClient, Db>>, MlsStoreError> {
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
                    stored_group.created_at_ns,
                )
            })
            .collect())
    }

    /// Look up a group by its ID
    ///
    /// Returns a [`MlsGroup`] if the group exists, or an error if it does not
    ///
    pub fn group(&self, group_id: &Vec<u8>) -> Result<MlsGroup<ApiClient, Db>, MlsStoreError> {
        let conn = self.context.db();
        let stored_group: Option<StoredGroup> = conn.fetch(group_id)?;
        stored_group
            .map(|g| MlsGroup::new(self.context.clone(), g.id, g.dm_id, g.created_at_ns))
            .ok_or(NotFound::GroupById(group_id.clone()))
            .map_err(Into::into)
    }
}
