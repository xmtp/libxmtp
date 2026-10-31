//! Local notification settings and the native notification API.

pub use crate::core::notifications::{
    NotificationChannel, NotificationConfig, NotificationOverride,
};
pub(crate) use crate::core::notifications::{
    decode, effective, enabled_from_record, encode, encode_config, notification_settings_changed,
    recipient_secret, stored_config,
};
use crate::{
    client::Client,
    context::XmtpSharedContext,
    worker::{WorkerKind, notifications as worker},
};
use serde::{Deserialize, Serialize};
use xmtp_common::{ErrorCode, RetryableError};
use xmtp_db::{
    StorageError, TransactionOutcome::Continue, XmtpMlsStorageProvider,
    notifications::StoredNotification, prelude::*,
};

/// The locally stored notification state. This getter does not make a request.
#[derive(Debug)]
pub enum NotificationState {
    Disabled,
    Enabled,
    Failed(NotificationError),
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

    fn rate_limit_delay(&self) -> Option<xmtp_common::time::Duration> {
        match self {
            Self::Api(e) => e.rate_limit_delay(),
            Self::Group(e) => e.rate_limit_delay(),
            _ => None,
        }
    }
}

impl From<xmtp_api::ApiError> for NotificationError {
    fn from(error: xmtp_api::ApiError) -> Self {
        use tonic::Code;
        // A rate limit is not the recipient topic limit: the request succeeds
        // after its delay, so the state stays enabled and adds keep going.
        // implements: PUSH-267
        if error.rate_limit_delay().is_some() {
            return Self::Api(error);
        }
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
    // implements: PUSH-266
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
        let valid_channel = match &config.channel {
            NotificationChannel::Apns { .. } | NotificationChannel::Fcm { .. } => true,
            NotificationChannel::Http { signing_key, .. } => {
                (16..=64).contains(&signing_key.as_slice().len())
            }
        };
        if !valid_channel {
            return Err(NotificationError::InvalidArgument);
        }
        let generation = crate::state_tx::state_write(self.context.mls_storage(), |tx| {
            let storage = tx.storage();
            let db = storage.db();
            let mut record = db.notification_record()?;
            if record.push_recipient_id.is_none() {
                record.push_recipient_id = Some(xmtp_common::rand_vec::<32>());
                record.push_recipient_secret =
                    Some(xmtp_cryptography::rand::rand_secret::<32>().into());
            }
            record.push_generation = record
                .push_generation
                .checked_add(1)
                .ok_or(StorageError::DbSerialize)?;
            record.push_config = Some(encode_config(&config)?);
            record.push_state = 1;
            record.push_failed_error = None;
            // A new configuration does not end a pending rate-limit delay.
            // Deadlines that cannot be decoded hold nothing, so they never
            // stop an app from enabling.
            let hold = worker::rate_limit_hold(&record).unwrap_or_default();
            record.push_deadlines = Some(encode(&hold)?);
            record.push_suppressed = None;
            db.save_notification_record(&record)?;
            Ok::<_, StorageError>(Continue(record.push_generation))
        })?
        .into_continued();
        notification_settings_changed(&self.context);
        let _guard = self
            .context
            .task_channels()
            .notification_request
            .lock()
            .await;
        let record = self.context.db().notification_record()?;
        if record.push_generation == generation && record.push_state == 1 {
            let result = worker::register(&self.context, &record, &config).await;
            if let Ok(true) = &result {
                worker::resume_after_registration(&self.context, generation)?;
            }
            drop(_guard);
            notification_settings_changed(&self.context);
            result?;
        }
        Ok(self.notification_state()?)
    }

    /// Disable locally before unregistering. The recipient identity and overrides stay.
    /// A failed unregister leaves the client disabled with its recipient reset owed.
    #[xmtp_common::rpc_span]
    pub async fn disable_notifications(&self) -> Result<(), NotificationError> {
        let record = crate::state_tx::state_write(self.context.mls_storage(), |tx| {
            let storage = tx.storage();
            let db = storage.db();
            // Deadlines that cannot be decoded hold nothing, so they never
            // stop an app from disabling.
            let hold = worker::rate_limit_hold(&db.notification_record()?).unwrap_or_default();
            let mut record = db.disable_notifications()?;
            // The task must not send inside a pending rate-limit delay after a
            // later enable. App calls themselves are not held (RATE-041 binds
            // the client's own retries).
            if hold.rate_limited(xmtp_common::time::now_ns()) {
                record.push_deadlines = Some(encode(&hold)?);
                db.save_notification_record(&record)?;
            }
            Ok::<_, StorageError>(Continue(record))
        })?
        .into_continued();
        notification_settings_changed(&self.context);
        if record.push_recipient_id.is_none() {
            return Ok(());
        }
        let _guard = self
            .context
            .task_channels()
            .notification_request
            .lock()
            .await;
        // A later disable still wants this recipient removed. The request
        // lock orders any subsequent enable's Register after Unregister.
        if self.context.db().notification_record()?.push_state != 0 {
            notification_settings_changed(&self.context);
            return Ok(());
        }
        worker::unregister(&self.context, &record, worker::request_deadline()).await
    }

    /// Read the locally stored state without a backend call.
    pub fn notification_state(&self) -> Result<NotificationState, StorageError> {
        state(&self.context.db().notification_record()?)
    }
}

#[cfg(test)]
mod tests;
