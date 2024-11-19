//! an "Intent" can be thought of as a commitment by an individual 'user', which drives
//! the state of a group chat forward.
//! Examples of an Intent:
//!     - Sending a message
//!     - Adding a member
//!     - Removing a member
//!
//! Intents are written to local storage (SQLite), before being published to the delivery service via gRPC. An
//! intent is fully resolved (success or failure) once it

use crate::{
    client::XmtpMlsLocalContext,
    retry::RetryableError,
    storage::{refresh_state::EntityKind, EncryptedMessageStore},
    xmtp_openmls_provider::XmtpOpenMlsProvider,
};
use std::{future::Future, sync::Arc};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProcessIntentError {
    #[error("[{0}] already processed")]
    AlreadyProcessed(u64),
    #[error("diesel error: {0}")]
    Diesel(#[from] diesel::result::Error),
    #[error("storage error: {0}")]
    Storage(#[from] crate::storage::StorageError),
}

impl RetryableError for ProcessIntentError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::AlreadyProcessed(_) => false,
            Self::Diesel(err) => err.is_retryable(),
            Self::Storage(err) => err.is_retryable(),
        }
    }
}

/// Intents holding the context of this Client
pub struct Intents {
    pub(crate) context: Arc<XmtpMlsLocalContext>,
}

impl Intents {
    pub(crate) fn store(&self) -> &EncryptedMessageStore {
        self.context.store()
    }

    /// Download all unread welcome messages and convert to groups.
    /// In a database transaction, increment the cursor for a given entity and
    /// apply the update after the provided `ProcessingFn` has completed successfully.
    pub(crate) async fn process_for_id<Fut, ProcessingFn, ReturnValue, ErrorType>(
        &self,
        entity_id: &Vec<u8>,
        entity_kind: EntityKind,
        cursor: u64,
        process_envelope: ProcessingFn,
    ) -> Result<ReturnValue, ErrorType>
    where
        Fut: Future<Output = Result<ReturnValue, ErrorType>>,
        ProcessingFn: FnOnce(XmtpOpenMlsProvider) -> Fut,
        ErrorType: From<diesel::result::Error>
            + From<crate::storage::StorageError>
            + From<ProcessIntentError>
            + std::fmt::Display,
    {
        self.store()
            .transaction_async(|provider| async move {
                let is_updated =
                    provider
                        .conn_ref()
                        .update_cursor(entity_id, entity_kind, cursor as i64)?;
                if !is_updated {
                    return Err(ProcessIntentError::AlreadyProcessed(cursor).into());
                }
                process_envelope(provider).await
            })
            .await
            .inspect(|_| {
                tracing::info!(
                    "Transaction completed successfully: process for entity [{:?}] envelope cursor[{}]",
                    entity_id,
                    cursor
            );
            })
            .inspect_err(|err| {
                tracing::info!(
                    "Transaction failed: process for entity [{:?}] envelope cursor[{}] error:[{}]",
                    entity_id,
                    cursor,
                    err
                );
            })
    }
}
