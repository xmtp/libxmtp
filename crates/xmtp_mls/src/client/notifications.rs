//! Local notification settings and the native notification API.

use crate::{
    client::Client,
    context::XmtpSharedContext,
    groups::MlsGroup,
    worker::{WorkerKind, notifications as worker},
};
use serde::{Deserialize, Serialize};
use xmtp_common::{ErrorCode, RetryableError};
use xmtp_db::consent_record::{ConsentState, ConsentType};
use xmtp_db::{
    StorageError, TransactionOutcome::Continue, XmtpMlsStorageProvider,
    notifications::StoredNotification, prelude::*,
};
use xmtp_proto::backend_v1::{self, register_request::Delivery};

/// The delivery endpoint. Credentials are omitted from debug output.
#[derive(Clone, Serialize, Deserialize)]
pub enum NotificationChannel {
    Apns { token: String },
    Fcm { token: String },
    Http { url: String, signing_key: Vec<u8> },
}

impl std::fmt::Debug for NotificationChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Apns { .. } => "Apns",
            Self::Fcm { .. } => "Fcm",
            Self::Http { .. } => "Http",
        })
    }
}

/// Delivery details and rules used to compute the desired subscriptions.
#[derive(Clone, Serialize, Deserialize)]
pub struct NotificationConfig {
    pub channel: NotificationChannel,
    pub consent_states: Vec<ConsentState>,
    pub include_welcomes: bool,
    pub include_sync_groups: bool,
    pub include_commits: bool,
    pub metadata: Vec<u8>,
}

impl NotificationConfig {
    /// Use the standard rules with the supplied delivery endpoint.
    pub fn new(channel: NotificationChannel) -> Self {
        Self {
            channel,
            consent_states: vec![ConsentState::Allowed],
            include_welcomes: true,
            include_sync_groups: false,
            include_commits: false,
            metadata: Vec::new(),
        }
    }

    pub(crate) fn channel_id(&self) -> i32 {
        match self.channel {
            NotificationChannel::Apns { .. } => backend_v1::Channel::Apns as i32,
            NotificationChannel::Fcm { .. } => backend_v1::Channel::Fcm as i32,
            NotificationChannel::Http { .. } => backend_v1::Channel::Http as i32,
        }
    }

    pub(crate) fn registration(&self, record: &StoredNotification) -> backend_v1::RegisterRequest {
        let delivery = match &self.channel {
            NotificationChannel::Apns { token } => Delivery::Apns(backend_v1::ApnsDelivery {
                token: token.clone(),
            }),
            NotificationChannel::Fcm { token } => Delivery::Fcm(backend_v1::FcmDelivery {
                token: token.clone(),
            }),
            NotificationChannel::Http { url, signing_key } => {
                Delivery::Http(backend_v1::HttpDelivery {
                    url: url.clone(),
                    signing_key: signing_key.clone(),
                })
            }
        };
        backend_v1::RegisterRequest {
            recipient_id: record.push_recipient_id.clone().unwrap_or_default(),
            recipient_secret: record.push_recipient_secret.clone().unwrap_or_default(),
            delivery: Some(delivery),
            metadata: self.metadata.clone(),
        }
    }
}

/// The locally stored notification state. This getter does not make a request.
#[derive(Debug)]
pub enum NotificationState {
    Disabled,
    Enabled,
    Failed(NotificationError),
}

/// A conversation rule. Sync groups use the client rule only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationOverride {
    Enabled,
    Disabled,
    Default,
}

/// Errors use fixed messages and never include notification credentials.
#[derive(thiserror::Error, ErrorCode)]
pub enum NotificationError {
    /// The task runner is disabled. Not retryable.
    #[error("notification task runner is disabled")]
    TaskRunnerDisabled,
    /// The recipient credential was rejected. Not retryable.
    #[error("notification permission denied")]
    PermissionDenied,
    /// The delivery configuration is invalid. Not retryable.
    #[error("notification configuration is invalid")]
    InvalidArgument,
    /// A notification value is outside the allowed range. Not retryable.
    #[error("notification value is out of range")]
    OutOfRange,
    /// The backend does not support notifications. Not retryable.
    #[error("notifications are not implemented")]
    Unimplemented,
    /// The delivery channel is not configured. Not retryable.
    #[error("notification channel is not configured")]
    ChannelNotConfigured,
    /// The recipient topic limit was reached. Retry after the desired set changes.
    #[error("notification topic limit reached")]
    ResourceExhausted,
    /// The notification request exceeded its time limit. Retryable.
    #[error("notification request timed out")]
    RequestTimeout,
    /// The recipient must register again. Retryable.
    #[error("notification recipient is not registered")]
    NotFound,
    /// A notification request failed. Retryable by the notification task.
    #[error("notification request failed")]
    #[error_code(inherit)]
    Api(#[source] xmtp_api::ApiError),
    #[error(transparent)]
    #[error_code(inherit)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    #[error_code(inherit)]
    Group(#[from] crate::groups::GroupError),
}

impl std::fmt::Debug for NotificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.error_code())
    }
}

impl RetryableError for NotificationError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::TaskRunnerDisabled | Self::ResourceExhausted => false,
            Self::Storage(e) => e.is_retryable(),
            Self::Group(e) => e.is_retryable(),
            _ => self.failure().is_none(),
        }
    }
}

impl From<xmtp_api::ApiError> for NotificationError {
    fn from(error: xmtp_api::ApiError) -> Self {
        use tonic::Code;
        let mut source: Option<&(dyn std::error::Error + 'static)> = Some(&error);
        while let Some(current) = source {
            if matches!(
                current.downcast_ref::<xmtp_proto::api::ApiClientError>(),
                Some(xmtp_proto::api::ApiClientError::Expired(_))
            ) {
                return Self::RequestTimeout;
            }
            source = current.source();
        }
        match xmtp_proto::api::grpc_status(&error).map(|status| status.code()) {
            Some(Code::PermissionDenied) => Self::PermissionDenied,
            Some(Code::InvalidArgument) => Self::InvalidArgument,
            Some(Code::OutOfRange) => Self::OutOfRange,
            Some(Code::Unimplemented) => Self::Unimplemented,
            Some(Code::FailedPrecondition) => Self::ChannelNotConfigured,
            Some(Code::ResourceExhausted) => Self::ResourceExhausted,
            Some(Code::NotFound) => Self::NotFound,
            _ => Self::Api(error),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) enum Failure {
    PermissionDenied,
    InvalidArgument,
    OutOfRange,
    Unimplemented,
    ChannelNotConfigured,
}

impl NotificationError {
    pub(crate) fn failure(&self) -> Option<Failure> {
        match self {
            Self::PermissionDenied => Some(Failure::PermissionDenied),
            Self::InvalidArgument => Some(Failure::InvalidArgument),
            Self::OutOfRange => Some(Failure::OutOfRange),
            Self::Unimplemented => Some(Failure::Unimplemented),
            Self::ChannelNotConfigured => Some(Failure::ChannelNotConfigured),
            _ => None,
        }
    }
}

impl From<Failure> for NotificationError {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::PermissionDenied => Self::PermissionDenied,
            Failure::InvalidArgument => Self::InvalidArgument,
            Failure::OutOfRange => Self::OutOfRange,
            Failure::Unimplemented => Self::Unimplemented,
            Failure::ChannelNotConfigured => Self::ChannelNotConfigured,
        }
    }
}

pub(crate) fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, StorageError> {
    serde_json::to_vec(value).map_err(|_| StorageError::DbSerialize)
}

pub(crate) fn decode<T: serde::de::DeserializeOwned>(value: &[u8]) -> Result<T, StorageError> {
    serde_json::from_slice(value).map_err(|_| StorageError::DbDeserialize)
}

pub(crate) fn state(record: &StoredNotification) -> Result<NotificationState, StorageError> {
    match record.push_state {
        0 => Ok(NotificationState::Disabled),
        1 => Ok(NotificationState::Enabled),
        2 => Ok(NotificationState::Failed(
            decode::<Failure>(
                record
                    .push_failed_error
                    .as_deref()
                    .ok_or(StorageError::DbDeserialize)?,
            )?
            .into(),
        )),
        _ => Err(StorageError::DbDeserialize),
    }
}

impl<Context: XmtpSharedContext> Client<Context> {
    /// Store configuration, then register inline. The task retries transient errors.
    #[xmtp_common::rpc_span]
    pub async fn enable_notifications(
        &self,
        config: NotificationConfig,
    ) -> Result<NotificationState, NotificationError> {
        if !self
            .context
            .worker_config()
            .worker_enabled(WorkerKind::TaskRunner)
        {
            return Err(NotificationError::TaskRunnerDisabled);
        }
        let generation = crate::state_tx::state_write(self.context.mls_storage(), |tx| {
            let storage = tx.storage();
            let db = storage.db();
            let mut record = db.notification_record()?;
            if record.push_recipient_id.is_none() {
                record.push_recipient_id = Some(xmtp_common::rand_vec::<32>());
                record.push_recipient_secret = Some(xmtp_common::rand_vec::<32>());
            }
            record.push_generation = record
                .push_generation
                .checked_add(1)
                .ok_or(StorageError::DbSerialize)?;
            record.push_config = Some(encode(&config)?);
            record.push_state = 1;
            record.push_failed_error = None;
            record.push_deadlines = Some(encode(&worker::Deadlines::default())?);
            record.push_suppressed = None;
            db.save_notification_record(&record)?;
            Ok::<_, StorageError>(Continue(record.push_generation))
        })?
        .into_continued();
        self.context.task_channels().wake_notifications();
        let _guard = self
            .context
            .task_channels()
            .notification_request
            .lock()
            .await;
        let record = self.context.db().notification_record()?;
        if record.push_generation == generation && record.push_state == 1 {
            let result = worker::register(&self.context, &record, &config).await;
            drop(_guard);
            self.context.task_channels().wake_notifications();
            result?;
        }
        Ok(self.notification_state()?)
    }

    /// Disable locally before unregistering. The recipient identity and overrides stay.
    #[xmtp_common::rpc_span]
    pub async fn disable_notifications(&self) -> Result<(), NotificationError> {
        let record = crate::state_tx::state_write(self.context.mls_storage(), |tx| {
            let storage = tx.storage();
            let db = storage.db();
            let mut record = db.notification_record()?;
            record.push_generation = record
                .push_generation
                .checked_add(1)
                .ok_or(StorageError::DbSerialize)?;
            record.push_state = 0;
            record.push_config = None;
            record.push_failed_error = None;
            record.push_deadlines = None;
            record.push_last_state = None;
            record.push_repairing = false;
            record.push_suppressed = None;
            db.clear_uploaded_topics()?;
            db.save_notification_record(&record)?;
            Ok::<_, StorageError>(Continue(record))
        })?
        .into_continued();
        self.context.task_channels().wake_notifications();
        if record.push_recipient_id.is_none() {
            return Ok(());
        }
        let _guard = self
            .context
            .task_channels()
            .notification_request
            .lock()
            .await;
        if self.context.db().notification_record()?.push_generation != record.push_generation {
            return Ok(());
        }
        let request = backend_v1::UnregisterRequest {
            recipient_id: record.push_recipient_id.unwrap_or_default(),
            recipient_secret: record.push_recipient_secret.unwrap_or_default(),
        };
        match worker::bounded(self.context.api().unregister(request)).await {
            Ok(_) | Err(NotificationError::NotFound) => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// Read the locally stored state without a backend call.
    pub fn notification_state(&self) -> Result<NotificationState, StorageError> {
        state(&self.context.db().notification_record()?)
    }
}

/// Resolve only local rules. Active membership is checked separately by the task.
pub(crate) fn effective(
    config: &NotificationConfig,
    group: &xmtp_db::group::StoredGroup,
    consent: ConsentState,
) -> bool {
    use xmtp_proto::types::ConversationType;
    if group.conversation_type == ConversationType::Sync {
        return config.include_sync_groups;
    }
    if !matches!(
        group.conversation_type,
        ConversationType::Group | ConversationType::Dm
    ) {
        return false;
    }
    match group.push_override {
        Some(1) => true,
        Some(0) => false,
        _ => config.consent_states.contains(&consent),
    }
}

impl<Context: XmtpSharedContext> MlsGroup<Context> {
    /// Set an override after which the task recomputes the desired set.
    pub fn set_notifications(&self, value: NotificationOverride) -> Result<(), StorageError> {
        crate::state_tx::state_write(self.context.mls_storage(), |tx| {
            tx.storage().db().set_notification_override(
                &self.group_id,
                match value {
                    NotificationOverride::Enabled => Some(1),
                    NotificationOverride::Disabled => Some(0),
                    NotificationOverride::Default => None,
                },
            )?;
            Ok::<_, StorageError>(Continue(()))
        })?;
        self.context.task_channels().wake_notifications();
        Ok(())
    }

    /// Return the effective local rule for this conversation.
    pub fn notifications_enabled(&self) -> Result<bool, StorageError> {
        let db = self.context.db();
        let record = db.notification_record()?;
        if record.push_state != 1 {
            return Ok(false);
        }
        let config: NotificationConfig = decode(
            record
                .push_config
                .as_deref()
                .ok_or(StorageError::DbDeserialize)?,
        )?;
        let group = db
            .find_group(&self.group_id)?
            .ok_or(xmtp_db::NotFound::GroupById(self.group_id))?;
        let consent = db
            .get_consent_record(hex::encode(self.group_id), ConsentType::ConversationId)?
            .map(|row| row.state)
            .unwrap_or(ConsentState::Unknown);
        Ok(effective(&config, &group, consent))
    }
}

#[cfg(test)]
mod tests;
