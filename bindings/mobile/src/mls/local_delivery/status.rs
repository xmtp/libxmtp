use xmtp_common::RetryableError;
use xmtp_mls::subscriptions::incoming::{
    IncomingConnection, IncomingError, IncomingProcessing, IncomingRegistration, IncomingStatus,
    IncomingTopicStatus,
};

/// State of the shared incoming connection, not the local reader.
#[derive(Clone, uniffi::Enum)]
pub enum FfiMessageConnection {
    Connecting,
    Connected,
    Reconnecting,
    Failed,
    Closed,
}

impl From<IncomingConnection> for FfiMessageConnection {
    fn from(value: IncomingConnection) -> Self {
        match value {
            IncomingConnection::Connecting => Self::Connecting,
            IncomingConnection::Connected => Self::Connected,
            IncomingConnection::Reconnecting => Self::Reconnecting,
            IncomingConnection::Failed => Self::Failed,
            IncomingConnection::Closed => Self::Closed,
        }
    }
}

/// Whether the receiver has registered this topic in the current scope.
#[derive(Clone, uniffi::Enum)]
pub enum FfiMessageRegistration {
    Pending,
    Active,
    Removed,
}

impl From<IncomingRegistration> for FfiMessageRegistration {
    fn from(value: IncomingRegistration) -> Self {
        match value {
            IncomingRegistration::Pending => Self::Pending,
            IncomingRegistration::Active => Self::Active,
            IncomingRegistration::Removed => Self::Removed,
        }
    }
}

/// Processing progress through fixed targets. App acknowledgement does not affect it.
#[derive(Clone, uniffi::Enum)]
pub enum FfiMessageProcessing {
    Pending,
    Complete,
    Blocked,
    Cancelled,
}

impl From<IncomingProcessing> for FfiMessageProcessing {
    fn from(value: IncomingProcessing) -> Self {
        match value {
            IncomingProcessing::Pending => Self::Pending,
            IncomingProcessing::Complete => Self::Complete,
            IncomingProcessing::Blocked => Self::Blocked,
            IncomingProcessing::Cancelled => Self::Cancelled,
        }
    }
}

/// A typed receiver cause. Use the code for decisions and the message for display.
#[derive(Clone, uniffi::Record)]
pub struct FfiMessageCatchUpError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl From<&IncomingError> for FfiMessageCatchUpError {
    fn from(error: &IncomingError) -> Self {
        Self {
            code: error.code().to_owned(),
            message: error.to_string(),
            retryable: error.is_retryable(),
        }
    }
}

/// One topic's fixed target and durable progress in a scope generation.
#[derive(Clone, uniffi::Record)]
pub struct FfiMessageTopicStatus {
    /// Complete encoded topic bytes, including the topic kind.
    pub topic: Vec<u8>,
    pub scope_generation: u64,
    pub registration: FfiMessageRegistration,
    /// Fixed network target H. None means no target has been captured yet.
    pub target: Option<u64>,
    /// Durable receipt cursor F.
    pub received: u64,
    /// Resolved processing cursor P, independent of local delivery acknowledgement D.
    pub processed: u64,
    /// Welcome rows at or below H that still need resolution.
    pub unresolved_welcomes: u64,
    pub processing: FfiMessageProcessing,
    pub blocked: Option<String>,
    pub error: Option<FfiMessageCatchUpError>,
}

impl From<IncomingTopicStatus> for FfiMessageTopicStatus {
    fn from(status: IncomingTopicStatus) -> Self {
        Self {
            topic: status.topic.into(),
            scope_generation: status.scope_generation,
            registration: status.registration.into(),
            target: status.target.map(|cursor| cursor.0),
            received: status.received.0,
            processed: status.processed.0,
            unresolved_welcomes: status.unresolved_welcomes,
            processing: status.processing.into(),
            blocked: status.blocked,
            error: status.error.as_deref().map(Into::into),
        }
    }
}

/// One scope generation. The prior generation is bounded separately, not recursively.
#[derive(Clone, uniffi::Record)]
pub struct FfiMessageCatchUpGeneration {
    /// Changes when the selected topic scope changes.
    pub scope_generation: u64,
    /// Changes when the receiver replaces its connection.
    pub connection_generation: u64,
    pub connection: FfiMessageConnection,
    pub topics: Vec<FfiMessageTopicStatus>,
    /// Starting or discovered Welcome work can still add group obligations.
    pub discovery_pending: bool,
    pub processing: FfiMessageProcessing,
    pub error: Option<FfiMessageCatchUpError>,
}

impl From<IncomingStatus> for FfiMessageCatchUpGeneration {
    fn from(status: IncomingStatus) -> Self {
        Self {
            scope_generation: status.scope_generation,
            connection_generation: status.connection_generation,
            connection: status.connection.into(),
            topics: status.topics.into_iter().map(Into::into).collect(),
            discovery_pending: status.discovery_pending,
            processing: status.processing.into(),
            error: status.error.as_deref().map(Into::into),
        }
    }
}

/// Current catch-up state and at most one replaced scope generation.
#[derive(Clone, uniffi::Record)]
pub struct FfiMessageCatchUpSnapshot {
    pub current: FfiMessageCatchUpGeneration,
    /// Retains the prior generation's outcome when selection changes.
    pub previous: Option<FfiMessageCatchUpGeneration>,
}

impl From<IncomingStatus> for FfiMessageCatchUpSnapshot {
    fn from(mut status: IncomingStatus) -> Self {
        let previous = status.previous.take().map(|status| (*status).into());
        Self {
            current: status.into(),
            previous,
        }
    }
}
