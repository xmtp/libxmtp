use super::Result;
use crate::context::XmtpSharedContext;
use crate::groups::GroupError;
use crate::groups::InitialMembershipValidator;
use crate::groups::welcome_sync::WelcomeService;
use crate::intents::ProcessIntentError;
use crate::{groups::MlsGroup, subscriptions::WelcomeOrGroup};
use std::collections::HashSet;
use xmtp_common::{Retry, retry_async};
use xmtp_db::{consent_record::ConsentState, group::ConversationType, prelude::*};
use xmtp_proto::types::{Cursor, WelcomeMessage};

/// Future for processing `WelcomeorGroup`
pub struct ProcessWelcomeFuture<Context> {
    /// welcome ids in DB and which are already processed
    known_welcome_ids: HashSet<Cursor>,
    /// The libxmtp client
    context: Context,
    /// the welcome or group being processed in this future
    item: WelcomeOrGroup,
    /// Conversation type to filter for, if any.
    conversation_type: Option<ConversationType>,
    /// To skip or include duplicate dms in the stream
    include_duplicate_dms: bool,
    /// Consent states to filter for, if any.
    consent_states: Option<Vec<ConsentState>>,
}

pub enum ProcessWelcomeResult<Context> {
    /// New Group and welcome id
    New {
        group: MlsGroup<Context>,
        id: Cursor,
    },
    /// A group we already have/we created that might not have a welcome id
    NewStored {
        group: MlsGroup<Context>,
        maybe_sequence_id: Option<i64>,
        maybe_originator: Option<i64>,
    },
    /// Skip this welcome but add and id to known welcome ids
    IgnoreId { id: Cursor },
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
    /// * `include_duplicate_dms` - Optional filter to include duplicate dms in the stream
    /// * `consent_states` - Optional filter for specific consent states
    ///
    /// # Returns
    /// * `Result<ProcessWelcomeFuture<C>>` - A new future for processing
    ///
    /// # Errors
    /// Returns an error if initialization fails
    ///
    /// # Example
    pub fn new(
        known_welcome_ids: HashSet<Cursor>,
        context: Context,
        item: WelcomeOrGroup,
        conversation_type: Option<ConversationType>,
        include_duplicate_dms: bool,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<ProcessWelcomeFuture<Context>> {
        Ok(Self {
            known_welcome_ids,
            context,
            item,
            conversation_type,
            include_duplicate_dms,
            consent_states,
        })
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
            Welcome(ref welcome) => {
                tracing::debug!("got welcome with id {}", welcome.cursor);
                // try to load it from store first and avoid overhead
                // of processing a welcome & erroring
                // for immediate return, this must stay in the top-level future,
                // to avoid a possible yield on the await in on_welcome.
                if self.known_welcome_ids.contains(&welcome.cursor) {
                    tracing::debug!(
                        "Found existing welcome. Returning from db & skipping processing"
                    );
                    if let Ok(Some(group)) = self.load_from_store(welcome.cursor) {
                        return self
                            .filter(ProcessWelcomeResult::New {
                                group,
                                id: welcome.cursor,
                            })
                            .await;
                    }
                }
                tracing::info!(
                    "could not find group for welcome {}, processing",
                    welcome.cursor
                );
                // sync welcome from the network
                if let Some(group) = self.on_welcome(welcome).await? {
                    ProcessWelcomeResult::New {
                        group,
                        id: welcome.cursor,
                    }
                } else {
                    tracing::info!("Oneshot welcome message processed, skipping stream event.");
                    ProcessWelcomeResult::IgnoreId { id: welcome.cursor }
                }
            }
            Group(ref id) => {
                tracing::info!("stream got existing group, pulling from db.");
                let (group, stored_group) = MlsGroup::new_cached(self.context.clone(), id)?;

                ProcessWelcomeResult::NewStored {
                    group,
                    maybe_sequence_id: stored_group.sequence_id,
                    maybe_originator: stored_group.originator_id,
                }
            }
        };
        self.filter(process_result).await
    }

    /// Checks whether a group should be included in the stream based on filtering rules.
    ///
    /// Returns `Ok(true)` if the group should be included in the stream,
    /// `Ok(false)` if it should be filtered out.
    ///
    /// # Arguments
    /// * `group` - The group to check
    /// * `check_virtual` - Whether to filter out virtual groups (only for new welcomes)
    ///
    /// # Filtering Rules
    /// 1. Virtual groups are filtered out only if `check_virtual` is true
    /// 2. Duplicate DMs are filtered out if `include_duplicate_dms` is false
    /// 3. Conversation type must match if a filter is specified
    /// 4. Consent state must match if a filter is specified
    ///
    /// # Errors
    /// Returns an error if retrieving group metadata or consent state fails
    async fn should_include_group(
        &self,
        group: &MlsGroup<Context>,
        check_virtual: bool,
    ) -> Result<bool> {
        let metadata = group.metadata().await?;

        // Filter out virtual groups (only for new welcomes, not stored groups)
        if check_virtual && metadata.conversation_type.is_virtual() {
            tracing::debug!("Virtual group welcome processed. Skipping stream.");
            return Ok(false);
        }

        // Filter out duplicate DMs if not included
        if !self.include_duplicate_dms
            && metadata.conversation_type == ConversationType::Dm
            && self.context.db().has_duplicate_dm(&group.group_id)?
        {
            tracing::debug!("Duplicate DM group detected. Skipping stream.");
            return Ok(false);
        }

        // Check conversation type filter
        let conversation_type_match = self
            .conversation_type
            .is_none_or(|ct| ct == metadata.conversation_type);

        // Check consent state filter
        let consent_state_match = if let Some(ref consent_states) = self.consent_states {
            consent_states.contains(&group.consent_state()?)
        } else {
            true
        };

        Ok(conversation_type_match && consent_state_match)
    }

    /// Applies conversation type and consent state filtering to processed welcome results.
    ///
    /// After a welcome message or group has been processed, this function determines
    /// whether it should be streamed to the client based on filtering rules.
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
    /// Returns an error if retrieving group metadata or applying filters fails
    async fn filter(
        &self,
        processed: ProcessWelcomeResult<Context>,
    ) -> Result<ProcessWelcomeResult<Context>> {
        use super::ProcessWelcomeResult::*;
        match processed {
            New { group, id } => {
                // For new welcomes, filter out virtual groups
                if self.should_include_group(&group, true).await? {
                    Ok(ProcessWelcomeResult::New { group, id })
                } else {
                    Ok(ProcessWelcomeResult::IgnoreId { id })
                }
            }
            NewStored {
                group,
                maybe_sequence_id,
                maybe_originator,
            } => {
                // For stored groups, don't filter out virtual groups
                if self.should_include_group(&group, false).await? {
                    Ok(ProcessWelcomeResult::NewStored {
                        group,
                        maybe_sequence_id,
                        maybe_originator,
                    })
                } else if let Some(id) = maybe_sequence_id
                    && let Some(originator) = maybe_originator
                {
                    Ok(ProcessWelcomeResult::IgnoreId {
                        id: Cursor {
                            sequence_id: id as u64,
                            originator_id: originator as u32,
                        },
                    })
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
    ///   - The MLS group associated with the welcome, if there is one
    ///   - The welcome ID for tracking
    ///
    /// # Errors
    /// Returns an error if synchronization fails or the group cannot be found
    ///
    /// # Note
    /// This function uses retry logic to handle transient network failures
    async fn on_welcome(&self, welcome: &WelcomeMessage) -> Result<Option<MlsGroup<Context>>> {
        let WelcomeMessage {
            cursor,
            created_ns: _,
            installation_key,
            ..
        } = welcome;
        let id = cursor.sequence_id as i64;

        tracing::info!(
            installation_id = hex::encode(installation_key),
            welcome_id = &id,
            "Trying to process streamed welcome"
        );
        self.process_welcome(welcome).await
    }

    async fn process_welcome(&self, welcome: &WelcomeMessage) -> Result<Option<MlsGroup<Context>>> {
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

        let id = welcome.cursor;
        if let Ok(maybe_group) = res {
            Ok(maybe_group)
        } else if let Err(GroupError::ProcessIntent(ProcessIntentError::WelcomeAlreadyProcessed(
            _,
        ))) = res
        {
            Ok(self.load_from_store(id)?)
        } else {
            Err(res.expect_err("Checked for Ok value").into())
        }
    }

    /// Load a group from disk by its welcome_id
    fn load_from_store(&self, cursor: Cursor) -> Result<Option<MlsGroup<Context>>> {
        let maybe_group = self.context.db().find_group_by_sequence_id(cursor)?;
        let Some(group) = maybe_group else {
            tracing::warn!(
                welcome_id = %cursor,
                "Already processed welcome not loaded from store (likely pre-existing group or oneshot message)"
            );
            return Ok(None);
        };
        tracing::info!(
            inbox_id = self.context.inbox_id(),
            group_id = hex::encode(&group.id),
            dm_id = group.dm_id,
            welcome_id = ?group.sequence_id,
            "loading existing group for welcome_id: {:?}",
            group.cursor()
        );
        Ok(Some(MlsGroup::new(
            self.context.clone(),
            group.id,
            group.dm_id,
            group.conversation_type,
            group.created_at_ns,
        )))
    }
}
