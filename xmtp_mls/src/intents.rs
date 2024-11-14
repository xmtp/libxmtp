//! an "Intent" can be thought of as a commitment by an individual 'user', which drives
//! the state of a group chat forward.
//! Examples of an Intent:
//!     - Sending a message
//!     - Adding a member
//!     - Removing a member
//!
//! Intents are written to local storage (SQLite), before being published to the delivery service via gRPC. An
//! intent is fully resolved (success or failure) once it

use std::{future::Future, sync::Arc};

use crate::{
    client::{MessageProcessingError, XmtpMlsLocalContext},
    storage::{refresh_state::EntityKind, EncryptedMessageStore},
    xmtp_openmls_provider::XmtpOpenMlsProvider,
};

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
    pub(crate) async fn process_for_id<Fut, ProcessingFn, ReturnValue>(
        &self,
        entity_id: &Vec<u8>,
        entity_kind: EntityKind,
        cursor: u64,
        process_envelope: ProcessingFn,
    ) -> Result<ReturnValue, MessageProcessingError>
    where
        Fut: Future<Output = Result<ReturnValue, MessageProcessingError>>,
        ProcessingFn: FnOnce(XmtpOpenMlsProvider) -> Fut,
    {
        self.store()
            .transaction_async(|provider| async move {
                let is_updated =
                    provider
                        .conn_ref()
                        .update_cursor(entity_id, entity_kind, cursor as i64)?;
                if !is_updated {
                    return Err(MessageProcessingError::AlreadyProcessed(cursor));
                }
                process_envelope(provider).await
            })
            .await
            .map(|result| {
                tracing::info!(
                    entity_id,
                    entity_kind,
                    cursor,
                    "Transaction completed successfully: process for entity [{}] envelope cursor[{}]",
                    entity_id,
                    cursor
                );
                result
            })
            .map_err(|err| {
                tracing::info!(
                    entity_id,
                    entity_kind,
                    cursor,
                    "Transaction failed: process for entity [{}] envelope cursor[{}]",
                    entity_id,
                    cursor
                );
                err
            })
    }
}
