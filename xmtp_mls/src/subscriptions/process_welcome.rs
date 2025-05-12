//! The future for processing a welcome from a stream

use super::{
    stream_conversations::{ConversationStreamError, ProcessWelcomeResult},
    Result, WelcomeOrGroup,
};
use crate::groups::{scoped_client::ScopedGroupClient, MlsGroup};
use xmtp_common::{retry_async, Retry};
use xmtp_db::{group::ConversationType, NotFound};

use std::collections::HashSet;
use xmtp_proto::{mls_v1::WelcomeMessage, xmtp::mls::api::v1::welcome_message};

fn extract_welcome_message(welcome: &WelcomeMessage) -> Result<&welcome_message::V1> {
    match welcome.version {
        Some(welcome_message::Version::V1(ref welcome)) => Ok(welcome),
        _ => Err(ConversationStreamError::InvalidPayload.into()),
    }
}

/// Future for processing `WelcomeorGroup`
pub struct ProcessWelcomeFuture<Client> {
    /// welcome ids in DB and which are already processed
    known_welcome_ids: HashSet<i64>,
    /// The libxmtp client
    client: Client,
    /// the welcome or group being processed in this future
    item: WelcomeOrGroup,
    /// Conversation type to filter for, if any.
    conversation_type: Option<ConversationType>,
}

impl<C> ProcessWelcomeFuture<C>
where
    C: ScopedGroupClient + Clone,
{
    /// Creates a new `ProcessWelcomeFuture` to handle processing of welcome messages or groups.
    ///
    /// This function initializes the future that will handle the core logic for
    /// processing a welcome message or group identifier. It captures all necessary
    /// context for the asynchronous processing operation.
    ///
    /// # Arguments
    /// * `known_welcome_ids` - Set of already processed welcome IDs for deduplication
    /// * `client` - The client to use for processing and database operations
    /// * `item` - The welcome message or group to process
    /// * `conversation_type` - Optional filter for specific conversation types
    ///
    /// # Returns
    /// * `Result<ProcessWelcomeFuture<C>>` - A new future for processing
    ///
    /// # Errors
    /// Returns an error if initialization fails
    ///
    /// # Example
    /// ```no_run
    /// let future = ProcessWelcomeFuture::new(
    ///     known_ids,
    ///     client.clone(),
    ///     WelcomeOrGroup::Welcome(welcome),
    ///     Some(ConversationType::Group),
    /// )?;
    /// let result = future.process().await?;
    /// ```
    pub(super) fn new(
        known_welcome_ids: HashSet<i64>,
        client: C,
        item: WelcomeOrGroup,
        conversation_type: Option<ConversationType>,
    ) -> Result<ProcessWelcomeFuture<C>> {
        Ok(Self {
            known_welcome_ids,
            client,
            item,
            conversation_type,
        })
    }
}

/// bulk of the processing for a new welcome/group
impl<C> ProcessWelcomeFuture<C>
where
    C: ScopedGroupClient + Clone,
{
    /// Processes a welcome message or group.
    ///
    /// handles new conversation events. It implements different processing paths for welcome
    /// messages versus group identifiers:
    ///
    /// For welcome messages:
    /// 1. Extracts the welcome payload and ID
    /// 2. Checks if the welcome has already been processed (fast path)
    /// 3. If not, triggers network synchronization for the welcome
    /// 4. Loads the resulting group from the database
    ///
    /// For groups:
    /// 1. Validates and loads the group from the database
    /// 2. Captures any associated welcome ID
    ///
    /// Finally, it applies conversation type filtering to determine if the
    /// conversation should be streamed to the client.
    ///
    /// # Returns
    /// * `Result<ProcessWelcomeResult<C>>` - The processing result indicating
    ///   how the welcome/group should be handled
    ///
    /// # Errors
    /// Returns an error if any step in the processing pipeline fails
    ///
    /// # Tracing
    #[tracing::instrument(skip_all)]
    pub async fn process(self) -> Result<ProcessWelcomeResult<C>> {
        use WelcomeOrGroup::*;
        let process_result = match self.item {
            Welcome(ref w) => {
                let welcome = extract_welcome_message(w)?;
                let id = welcome.id as i64;
                tracing::debug!("got welcome with id {}", id);
                // try to load it from store first and avoid overhead
                // of processing a welcome & erroring
                // for immediate return, this must stay in the top-level future,
                // to avoid a possible yield on the await in on_welcome.
                if self.known_welcome_ids.contains(&id) {
                    tracing::debug!(
                        "Found existing welcome. Returning from db & skipping processing"
                    );
                    let (group, id) = self.load_from_store(id)?;
                    return self.filter(ProcessWelcomeResult::New { group, id }).await;
                }
                // sync welcome from the network
                let (group, id) = self.on_welcome(welcome).await?;
                ProcessWelcomeResult::New { group, id }
            }
            Group(ref id) => {
                tracing::debug!("Stream conversations got existing group, pulling from db.");
                let (group, stored_group) =
                    MlsGroup::new_validated(self.client.clone(), id.to_vec())?;

                ProcessWelcomeResult::NewStored {
                    group,
                    maybe_id: stored_group.welcome_id,
                }
            }
        };
        self.filter(process_result).await
    }

    /// Applies conversation type filtering to processed welcome results.
    ///
    /// After a welcome message or group has been processed, this function determines
    /// whether it should be streamed to the client based on:
    ///
    /// 1. Duplicate DM detection - prevents streaming multiple DMs for the same conversation
    /// 2. Conversation type matching - filters based on the requested conversation type
    ///
    /// The function modifies the `ProcessWelcomeResult` to indicate whether the
    /// conversation should be streamed, ignored but tracked, or completely ignored.
    ///
    /// # Arguments
    /// * `processed` - The initial processing result from handling the welcome/group
    ///
    /// # Returns
    /// * `Result<ProcessWelcomeResult<C>>` - The filtered result, potentially
    ///   changing the handling instruction
    ///
    /// # Errors
    /// Returns an error if retrieving group metadata fails
    async fn filter(&self, processed: ProcessWelcomeResult<C>) -> Result<ProcessWelcomeResult<C>> {
        use super::ProcessWelcomeResult::*;
        match processed {
            New { group, id } => {
                let metadata = group.metadata().await?;
                // If it's a duplicate DM, don’t stream
                if metadata.conversation_type == ConversationType::Dm
                    && self.client.db().has_duplicate_dm(&group.group_id)?
                {
                    tracing::debug!("Duplicate DM group detected from Group(id). Skipping stream.");
                    return Ok(ProcessWelcomeResult::IgnoreId { id });
                }

                if self
                    .conversation_type
                    .is_none_or(|ct| ct == metadata.conversation_type)
                {
                    Ok(ProcessWelcomeResult::New { group, id })
                } else {
                    Ok(ProcessWelcomeResult::IgnoreId { id })
                }
            }
            NewStored { group, maybe_id } => {
                let metadata = group.metadata().await?;
                // If it's a duplicate DM, don’t stream
                if metadata.conversation_type == ConversationType::Dm
                    && self.client.db().has_duplicate_dm(&group.group_id)?
                {
                    tracing::debug!("Duplicate DM group detected from Group(id). Skipping stream.");
                    if let Some(id) = maybe_id {
                        return Ok(ProcessWelcomeResult::IgnoreId { id });
                    } else {
                        return Ok(ProcessWelcomeResult::Ignore);
                    }
                }

                if self
                    .conversation_type
                    .is_none_or(|ct| ct == metadata.conversation_type)
                {
                    Ok(ProcessWelcomeResult::NewStored { group, maybe_id })
                } else if let Some(id) = maybe_id {
                    Ok(ProcessWelcomeResult::IgnoreId { id })
                } else {
                    Ok(ProcessWelcomeResult::Ignore)
                }
            }
            other => Ok(other),
        }
    }

    /// Processes a new welcome message by syncing with the network.
    ///
    /// This method handles the synchronization of a welcome message with the network,
    /// retrieving the associated group data. The process involves:
    ///
    /// 1. Extracting metadata from the welcome message
    /// 2. Logging the processing attempt
    /// 3. Triggering welcome synchronization with retry logic
    /// 4. Loading the resulting group from the database
    ///
    /// # Arguments
    /// * `welcome` - The welcome message (V1) to process
    ///
    /// # Returns
    /// * `Result<(MlsGroup<C>, i64)>` - A tuple containing:
    ///   - The MLS group associated with the welcome
    ///   - The welcome ID for tracking
    ///
    /// # Errors
    /// Returns an error if synchronization fails or the group cannot be found
    ///
    /// # Note
    /// This function uses retry logic to handle transient network failures
    async fn on_welcome(&self, welcome: &welcome_message::V1) -> Result<(MlsGroup<C>, i64)> {
        let welcome_message::V1 {
            id,
            created_ns: _,
            ref installation_key,
            ..
        } = welcome;
        let id = *id as i64;

        let Self { ref client, .. } = self;
        tracing::info!(
            installation_id = hex::encode(installation_key),
            welcome_id = &id,
            "Trying to process streamed welcome"
        );

        retry_async!(Retry::default(), (async { client.sync_welcomes().await }))?;

        self.load_from_store(id)
    }

    /// Load a group from disk by its welcome_id
    fn load_from_store(&self, id: i64) -> Result<(MlsGroup<C>, i64)> {
        let provider = self.client.mls_provider();
        let group = provider
            .db()
            .find_group_by_welcome_id(id)?
            .ok_or(NotFound::GroupByWelcome(id))?;
        tracing::info!(
            inbox_id = self.client.inbox_id(),
            group_id = hex::encode(&group.id),
            dm_id = group.dm_id,
            welcome_id = ?group.welcome_id,
            "loading existing group for welcome_id: {:?}",
            group.welcome_id
        );
        Ok((
            MlsGroup::new(
                self.client.clone(),
                group.id,
                group.dm_id,
                group.created_at_ns,
            ),
            id,
        ))
    }
}
