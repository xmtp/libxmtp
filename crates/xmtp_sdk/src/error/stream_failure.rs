use xmtp_mls::subscriptions::stream_failure as wire;

/// The operation with unfinished fixed processing obligations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum StreamFailureKind {
    Barrier,
    PublishedButUnconfirmed,
    CatchUp,
}

/// Why a barrier stopped waiting. Pending work remains stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum StreamBarrierReason {
    Blocked,
    Deadline,
    Cancelled,
}

/// The typed cause of one unfinished topic obligation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum StreamBarrierCauseKind {
    TargetPending,
    ReceiptPending,
    ProcessingPending,
    Blocked,
    Storage,
    Receiver,
    InvalidTopic,
}

/// Safe cause fields. Use the kind and code for control flow.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct StreamBarrierCause {
    pub kind: StreamBarrierCauseKind,
    pub code: Option<String>,
    pub message: String,
    pub retryable: bool,
}

/// One fixed topic obligation and its stored processing progress.
#[xmtp_macro::sdk_export]
#[derive(Clone, PartialEq, Eq, uniffi::Record)]
pub struct StreamBarrierTopic {
    /// Complete topic bytes.
    #[sdk(redact)]
    pub topic: Vec<u8>,
    /// Captured owning scope. None precedes admission.
    #[uniffi(default = None)]
    #[sdk(shown)]
    pub scope_generation: Option<u64>,
    /// Target H. None differs from a captured empty target of zero.
    #[sdk(shown)]
    pub target: Option<u64>,
    /// Stored receipt cursor F.
    #[sdk(shown)]
    pub received: u64,
    /// Resolved processing cursor P, separate from consumer acknowledgement.
    #[sdk(shown)]
    pub processed: u64,
    /// Unresolved Welcome sequence IDs at or below H.
    #[sdk(shown)]
    pub unresolved_welcomes: Vec<u64>,
    #[sdk(shown)]
    pub inactive: bool,
    #[sdk(shown)]
    pub cause: Option<StreamBarrierCause>,
}

// Keep complete identifiers in structured data, outside diagnostic text.
impl StreamBarrierTopic {
    fn redacted_debug(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StreamBarrierTopic")
            .field("topic", &"<redacted>")
            .field("scope_generation", &self.scope_generation)
            .field("target", &self.target)
            .field("received", &self.received)
            .field("processed", &self.processed)
            .field("unresolved_welcomes", &self.unresolved_welcomes)
            .field("inactive", &self.inactive)
            .field("cause", &self.cause)
            .finish()
    }
}

/// Every unfinished topic from one failed barrier.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct StreamBarrierFailure {
    pub reason: StreamBarrierReason,
    pub unfinished: Vec<StreamBarrierTopic>,
}

/// Catch-up progress that remains committed after an incomplete result.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct StreamFailureSummary {
    pub messages: u64,
    pub conversations: u64,
    pub failed: u64,
    pub completed: bool,
}

/// Typed details from the core error, including all sibling barriers.
// implements: PROC-018
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct StreamFailureDetails {
    pub kind: StreamFailureKind,
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub intent_id: Option<i32>,
    pub published_intent_ids: Vec<i32>,
    pub summary: Option<StreamFailureSummary>,
    pub barriers: Vec<StreamBarrierFailure>,
}

#[derive(Debug, thiserror::Error)]
enum StreamFailureProjectionError {
    #[error("invalid topic encoding in core stream failure")]
    Topic(#[from] hex::FromHexError),
    #[error("invalid integer encoding in core stream failure")]
    Integer(#[from] std::num::ParseIntError),
}

impl From<wire::StreamBarrierCause> for StreamBarrierCause {
    fn from(cause: wire::StreamBarrierCause) -> Self {
        use wire::StreamBarrierCauseKind as Kind;
        Self {
            kind: match cause.kind {
                Kind::TargetPending => StreamBarrierCauseKind::TargetPending,
                Kind::ReceiptPending => StreamBarrierCauseKind::ReceiptPending,
                Kind::ProcessingPending => StreamBarrierCauseKind::ProcessingPending,
                Kind::Blocked => StreamBarrierCauseKind::Blocked,
                Kind::Storage => StreamBarrierCauseKind::Storage,
                Kind::Receiver => StreamBarrierCauseKind::Receiver,
                Kind::InvalidTopic => StreamBarrierCauseKind::InvalidTopic,
            },
            code: cause.code,
            message: cause.message,
            retryable: cause.retryable,
        }
    }
}

impl StreamBarrierTopic {
    fn from_wire(topic: wire::StreamBarrierTopic) -> Result<Self, StreamFailureProjectionError> {
        Ok(Self {
            topic: hex::decode(topic.topic)?,
            scope_generation: topic
                .scope_generation
                .map(|value| value.parse())
                .transpose()?,
            target: topic.target.map(|value| value.parse()).transpose()?,
            received: topic.received.parse()?,
            processed: topic.processed.parse()?,
            unresolved_welcomes: topic
                .unresolved_welcomes
                .into_iter()
                .map(|value| value.parse())
                .collect::<Result<_, _>>()?,
            inactive: topic.inactive,
            cause: topic.cause.map(Into::into),
        })
    }
}

impl StreamBarrierFailure {
    fn from_wire(
        failure: wire::StreamBarrierFailure,
    ) -> Result<Self, StreamFailureProjectionError> {
        Ok(Self {
            reason: match failure.reason {
                wire::StreamBarrierReason::Blocked => StreamBarrierReason::Blocked,
                wire::StreamBarrierReason::Deadline => StreamBarrierReason::Deadline,
                wire::StreamBarrierReason::Cancelled => StreamBarrierReason::Cancelled,
            },
            unfinished: failure
                .unfinished
                .into_iter()
                .map(StreamBarrierTopic::from_wire)
                .collect::<Result<_, _>>()?,
        })
    }
}

impl StreamFailureDetails {
    fn from_wire(
        failure: wire::StreamFailureDetails,
    ) -> Result<Self, StreamFailureProjectionError> {
        let summary = failure
            .summary
            .map(|summary| {
                Ok::<_, StreamFailureProjectionError>(StreamFailureSummary {
                    messages: summary.messages.parse()?,
                    conversations: summary.conversations.parse()?,
                    failed: summary.failed.parse()?,
                    completed: summary.completed,
                })
            })
            .transpose()?;
        Ok(Self {
            kind: match failure.kind {
                wire::StreamFailureKind::Barrier => StreamFailureKind::Barrier,
                wire::StreamFailureKind::PublishedButUnconfirmed => {
                    StreamFailureKind::PublishedButUnconfirmed
                }
                wire::StreamFailureKind::CatchUp => StreamFailureKind::CatchUp,
            },
            code: failure.code,
            message: failure.message,
            retryable: failure.retryable,
            intent_id: failure.intent_id,
            published_intent_ids: failure.published_intent_ids,
            summary,
            barriers: failure
                .barriers
                .into_iter()
                .map(StreamBarrierFailure::from_wire)
                .collect::<Result<_, _>>()?,
        })
    }
}

impl XmtpError {
    fn details_mut(&mut self) -> &mut ErrorDetails {
        match self {
            Self::ClientClosed(details)
            | Self::InvalidInput(details)
            | Self::StorageLocationRequired(details)
            | Self::IdentityNotFound(details)
            | Self::StorageBusy(details)
            | Self::Signer(details)
            | Self::Credential(details)
            | Self::ConfigurationUnavailable(details)
            | Self::ConfigurationInvalid(details)
            | Self::BackendMismatch(details)
            | Self::ClientVersionTooOld(details)
            | Self::AuthRequired(details)
            | Self::ChainNotAccepted(details)
            | Self::CredentialRejected(details)
            | Self::CredentialCallbackFailed(details)
            | Self::CallbackFailed(details)
            | Self::CredentialExhausted(details)
            | Self::CredentialMissing(details)
            | Self::PermissionDenied(details)
            | Self::InvalidArgument(details)
            | Self::OutOfRange(details)
            | Self::Unimplemented(details)
            | Self::ChannelNotConfigured(details)
            | Self::TaskRunnerDisabled(details)
            | Self::ResourceExhausted(details)
            | Self::RequestTimeout(details)
            | Self::NotificationNotFound(details)
            | Self::NotificationApi(details)
            | Self::NotificationStorage(details)
            | Self::NotificationGroup(details)
            | Self::RecoveryExhausted(details)
            | Self::Storage(details)
            | Self::Lagged(details)
            | Self::ConsumerOwned(details)
            | Self::InvalidCursor(details)
            | Self::ForeignCursor(details)
            | Self::StorageLocation(details)
            | Self::Attachment(details, _)
            | Self::IdentityMismatch(details)
            | Self::UnknownField(details)
            | Self::NotUserField(details)
            | Self::DuplicateField(details)
            | Self::UnsupportedType(details)
            | Self::TypeMismatch(details)
            | Self::TypeChanged(details)
            | Self::CodecEncodeFailed(details)
            | Self::CodecNotFound(details)
            | Self::CodecDecodeFailed(details)
            | Self::MalformedEnvelope(details)
            | Self::PublishedButUnconfirmed(details)
            | Self::UserLimitExceeded(details)
            | Self::Cancelled(details)
            | Self::Unknown(details) => details,
        }
    }

    fn with_stream_failure(mut self, error: &(dyn std::error::Error + 'static)) -> Self {
        let mut current = Some(error);
        while let Some(error) = current {
            if let Some(failure) = wire::stream_failure_details(error) {
                self.details_mut().stream_failure = xmtp_common::optify!(
                    StreamFailureDetails::from_wire(failure),
                    "could not project core stream failure"
                );
                break;
            }
            current = wrappers::wrapped(error).or_else(|| error.source());
        }
        self
    }
}
