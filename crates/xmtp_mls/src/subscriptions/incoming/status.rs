use std::sync::Arc;
use xmtp_common::RetryableError;
use xmtp_proto::types::{Cursor, Topic};

/// Transport health; a connected wire does not imply completed processing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IncomingConnection {
    Connecting,
    Connected,
    Reconnecting,
    Failed,
    Closed,
}

/// Whether this topic still has network interest for the current scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IncomingRegistration {
    Pending,
    Active,
    Removed,
}

/// Processing state through the fixed targets, independent of application delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IncomingProcessing {
    Pending,
    Complete,
    Blocked,
    Cancelled,
}

#[derive(Debug, thiserror::Error)]
pub enum IncomingError {
    #[error(transparent)]
    Storage(#[from] xmtp_db::StorageError),
    #[error(transparent)]
    Store(#[from] crate::mls_store::MlsStoreError),
    #[error(transparent)]
    Transport(#[from] xmtp_proto::api::NetworkError),
    #[error(transparent)]
    Group(#[from] crate::groups::GroupError),
    #[error(transparent)]
    Processing(#[from] crate::groups::mls_sync::GroupMessageProcessingError),
    #[error(transparent)]
    Identity(#[from] crate::identity_updates::IdentityDependencyError),
    #[error("unsupported incoming topic")]
    UnsupportedTopic,
}

impl RetryableError for IncomingError {
    fn is_retryable(&self) -> bool {
        match self {
            // The controller repairs this refusal with an ordered read from F.
            // Raw storage callers must still treat the omitted prefix as invalid.
            Self::Storage(xmtp_db::StorageError::Stream(
                xmtp_db::stream_storage::StreamStorageError::MissingPrefix { .. },
            ))
            | Self::Store(crate::mls_store::MlsStoreError::Storage(
                xmtp_db::StorageError::Stream(
                    xmtp_db::stream_storage::StreamStorageError::MissingPrefix { .. },
                ),
            )) => true,
            Self::Storage(error) => error.is_retryable(),
            Self::Store(error) => error.is_retryable(),
            Self::Transport(error) => error.is_retryable(),
            Self::Group(error) => error.is_retryable(),
            Self::Processing(error) => error.is_retryable(),
            Self::Identity(error) => error.is_retryable(),
            Self::UnsupportedTopic => false,
        }
    }
}

impl crate::worker::NeedsDbReconnect for IncomingError {
    fn needs_db_reconnect(&self) -> bool {
        match self {
            Self::Storage(error) => error.db_needs_connection(),
            Self::Store(error) => error.needs_db_reconnect(),
            Self::Group(error) => error.needs_db_reconnect(),
            Self::Processing(error) => error.needs_db_reconnect(),
            Self::Identity(error) => error.needs_db_reconnect(),
            Self::Transport(_) | Self::UnsupportedTopic => false,
        }
    }
}

impl IncomingError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Storage(_) => "incoming_storage",
            Self::Store(_) => "incoming_receive",
            Self::Transport(_) => "incoming_transport",
            Self::Group(_) => "incoming_welcome",
            Self::Processing(error) => error.processing_code(),
            Self::Identity(_) => "incoming_identity",
            Self::UnsupportedTopic => "unsupported_topic",
        }
    }
}

/// Per-topic proof of durable receipt and processing through one fixed head.
#[derive(Clone, Debug)]
pub struct IncomingTopicStatus {
    /// Topic whose progress is reported.
    pub topic: Topic,
    /// Scope revision that owns this target; old revisions cannot complete a new scope.
    pub scope_generation: u64,
    /// Registration is separate from both transport health and processing.
    pub registration: IncomingRegistration,
    /// Fixed head H; `None` means no target has been established yet.
    pub target: Option<Cursor>,
    /// Durable received prefix F, including retained pending work.
    pub received: Cursor,
    /// Durable processed prefix P, including terminal rejections.
    pub processed: Cursor,
    /// Unresolved Welcomes at or below H; later successes cannot hide them.
    pub unresolved_welcomes: u64,
    /// Completion predicate for this topic, not an application acknowledgement.
    pub processing: IncomingProcessing,
    /// Stable reason code when retained work cannot currently proceed.
    pub blocked: Option<String>,
    /// Latest error for this topic; sibling topics can still make progress.
    pub error: Option<Arc<IncomingError>>,
}

/// Current scope status plus the immediately replaced scope's cancellation result.
#[derive(Clone, Debug)]
pub struct IncomingStatus {
    /// Changes when the caller replaces the selected scope.
    pub scope_generation: u64,
    /// Changes when network receipt is opened again, without replacing fixed heads.
    pub connection_generation: u64,
    /// Health of the shared receiver, not proof of processing completion.
    pub connection: IncomingConnection,
    /// Independent fixed-target obligations for this scope.
    pub topics: Vec<IncomingTopicStatus>,
    /// The scope can still discover enrolled groups through its Welcome target.
    pub discovery_pending: bool,
    /// Aggregate processing result for the scope.
    pub processing: IncomingProcessing,
    /// Latest shared receiver or discovery error.
    pub error: Option<Arc<IncomingError>>,
    /// Only the immediately replaced generation is retained.
    pub previous: Option<Box<IncomingStatus>>,
}

impl IncomingStatus {
    pub(super) fn pending(generation: u64) -> Self {
        Self {
            scope_generation: generation,
            connection_generation: 0,
            connection: IncomingConnection::Connecting,
            topics: Vec::new(),
            discovery_pending: true,
            processing: IncomingProcessing::Pending,
            error: None,
            previous: None,
        }
    }

    pub(super) fn cancelled(generation: u64) -> Self {
        let mut status = Self::pending(generation);
        status.cancel();
        status
    }

    pub(crate) fn cancel(&mut self) {
        self.connection = IncomingConnection::Closed;
        self.processing = IncomingProcessing::Cancelled;
        self.discovery_pending = false;
        for topic in &mut self.topics {
            topic.registration = IncomingRegistration::Removed;
            topic.processing = IncomingProcessing::Cancelled;
        }
    }

    pub(super) fn without_previous(&self) -> Self {
        let mut status = self.clone();
        status.previous = None;
        status
    }
}
