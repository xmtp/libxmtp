use super::{Result, stream_conversations::ConversationStreamError};
use crate::context::XmtpSharedContext;
use crate::groups::GroupError;
use crate::groups::InitialMembershipValidator;
use crate::groups::welcome_sync::WelcomeService;
use crate::intents::ProcessIntentError;
use crate::{groups::MlsGroup, subscriptions::WelcomeOrGroup};
use std::collections::HashSet;
use xmtp_common::{Retry, retry_async};
use xmtp_db::{NotFound, group::ConversationType, prelude::*};
use xmtp_proto::mls_v1::{WelcomeMessage, welcome_message};

/// Future for processing `WelcomeorGroup`
pub struct ProcessWelcomeFuture<Context> {
    /// welcome ids in DB and which are already processed
    known_welcome_ids: HashSet<i64>,
    /// The libxmtp client
    context: Context,
    /// the welcome or group being processed in this future
    item: WelcomeOrGroup,
    /// Conversation type to filter for, if any.
    conversation_type: Option<ConversationType>,
}

pub enum ProcessWelcomeResult<Context> {
    /// New Group and welcome id
    New { group: MlsGroup<Context>, id: i64 },
    /// A group we already have/we created that might not have a welcome id
    NewStored {
        group: MlsGroup<Context>,
        maybe_id: Option<i64>,
    },
    /// Skip this welcome but add and id to known welcome ids
    IgnoreId { id: i64 },
    /// Skip this payload
    Ignore,
}

impl<Context> ProcessWelcomeFuture<Context>
where
    Context: XmtpSharedContext,
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
    pub fn new(
        known_welcome_ids: HashSet<i64>,
        context: Context,
        item: WelcomeOrGroup,
        conversation_type: Option<ConversationType>,
    ) -> Result<ProcessWelcomeFuture<Context>> {
        Ok(Self {
            known_welcome_ids,
            context,
            item,
            conversation_type,
        })
    }
}

fn extract_welcome_message(welcome: &WelcomeMessage) -> Result<&welcome_message::V1> {
    match welcome.version {
        Some(welcome_message::Version::V1(ref welcome)) => Ok(welcome),
        _ => Err(ConversationStreamError::InvalidPayload.into()),
    }
}

/// bulk of the processing for a new welcome/group
impl<Context> ProcessWelcomeFuture<Context>
where
    Context: XmtpSharedContext,
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
    pub async fn process(self) -> Result<ProcessWelcomeResult<Context>> {
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
                    if let Ok((group, id)) = self.load_from_store(id) {
                        return self.filter(ProcessWelcomeResult::New { group, id }).await;
                    }
                }
                tracing::info!("could not find group for welcome {}, processing", id);
                // sync welcome from the network
                let (group, id) = self.on_welcome(welcome).await?;
                ProcessWelcomeResult::New { group, id }
            }
            Group(ref id) => {
                tracing::info!("stream got existing group, pulling from db.");
                let (group, stored_group) = MlsGroup::new_cached(self.context.clone(), id)?;

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
    async fn filter(
        &self,
        processed: ProcessWelcomeResult<Context>,
    ) -> Result<ProcessWelcomeResult<Context>> {
        use super::ProcessWelcomeResult::*;
        match processed {
            New { group, id } => {
                let metadata = group.metadata().await?;

                // Do not stream sync groups.
                if metadata.conversation_type == ConversationType::Sync {
                    tracing::debug!("Sync group welcome processed. Skipping stream.");
                    return Ok(ProcessWelcomeResult::IgnoreId { id });
                }

                // If it's a duplicate DM, don’t stream
                if metadata.conversation_type == ConversationType::Dm
                    && self.context.db().has_duplicate_dm(&group.group_id)?
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
                    && self.context.db().has_duplicate_dm(&group.group_id)?
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
    /// * `Result<(MlsGroup<Context>, i64)>` - A tuple containing:
    ///   - The MLS group associated with the welcome
    ///   - The welcome ID for tracking
    ///
    /// # Errors
    /// Returns an error if synchronization fails or the group cannot be found
    ///
    /// # Note
    /// This function uses retry logic to handle transient network failures
    async fn on_welcome(&self, welcome: &welcome_message::V1) -> Result<(MlsGroup<Context>, i64)> {
        let welcome_message::V1 {
            id,
            created_ns: _,
            installation_key,
            ..
        } = welcome;
        let id = *id as i64;

        tracing::info!(
            installation_id = hex::encode(installation_key),
            welcome_id = &id,
            "Trying to process streamed welcome"
        );
        self.process_welcome(welcome).await
    }

    async fn process_welcome(
        &self,
        welcome: &welcome_message::V1,
    ) -> Result<(MlsGroup<Context>, i64)> {
        let welcomes = WelcomeService::new(self.context.clone());
        let res = retry_async!(
            Retry::default(),
            (async {
                let validator = InitialMembershipValidator::new(&self.context);
                welcomes
                    .process_new_welcome(welcome, false, validator)
                    .await
            })
        );

        if let Ok(_)
        | Err(GroupError::ProcessIntent(ProcessIntentError::WelcomeAlreadyProcessed(_))) = res
        {
            self.load_from_store(welcome.id as i64)
        } else {
            Err(res.expect_err("Checked for Ok value").into())
        }
    }

    /// Load a group from disk by its welcome_id
    fn load_from_store(&self, id: i64) -> Result<(MlsGroup<Context>, i64)> {
        let group = self
            .context
            .db()
            .find_group_by_welcome_id(id)?
            .ok_or(NotFound::GroupByWelcome(id))?;
        tracing::info!(
            inbox_id = self.context.inbox_id(),
            group_id = hex::encode(&group.id),
            dm_id = group.dm_id,
            welcome_id = ?group.welcome_id,
            "loading existing group for welcome_id: {:?}",
            group.welcome_id
        );
        Ok((
            MlsGroup::new(
                self.context.clone(),
                group.id,
                group.dm_id,
                group.conversation_type,
                group.created_at_ns,
            ),
            id,
        ))
    }
}
