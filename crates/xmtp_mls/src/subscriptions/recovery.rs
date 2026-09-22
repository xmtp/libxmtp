//! A recovery budget belongs to one public stream, never to the shared receiver.

use super::{incoming::IncomingError, local_delivery::LocalDeliveryError};
use std::sync::Arc;
use xmtp_common::time::{Duration, Instant};
use xmtp_proto::types::Topic;

const MAX_FAILURES: u64 = 10;
const MAX_OUTAGE: Duration = Duration::from_secs(10 * 60);
pub(crate) const HEALTHY_PERIOD: Duration = Duration::from_secs(30);
pub(crate) const RECOVERY_POLL: Duration = Duration::from_secs(1);

/// Source retryability describes one response. Explicit remote cancellation,
/// authorization refusals, and configuration latches end recovery immediately.
// implements: PROC-021, AUTH-022, AUTH-025, CONF-022, API-284
fn terminal_source(error: &(dyn std::error::Error + 'static)) -> bool {
    use xmtp_common::RetryableError;
    use xmtp_proto::api::{ApiClientError, AuthError};

    if let Some(status) = error.downcast_ref::<tonic::Status>() {
        return match status.code() {
            tonic::Code::PermissionDenied
            | tonic::Code::InvalidArgument
            | tonic::Code::OutOfRange
            | tonic::Code::Unimplemented => true,
            // Reuse the typed TimeoutExpired/hyper cancellation distinction.
            // A message that merely names a timeout is still remote cancellation.
            tonic::Code::Cancelled => {
                !xmtp_api_grpc::error::GrpcError::Status(status.clone()).is_retryable()
            }
            _ => false,
        };
    }
    let auth = error.downcast_ref::<AuthError>().copied().or_else(|| {
        match error.downcast_ref::<ApiClientError>() {
            Some(ApiClientError::Auth(auth)) => Some(*auth),
            _ => match error.downcast_ref::<xmtp_api::ApiError>() {
                Some(xmtp_api::ApiError::Auth(auth)) => Some(*auth),
                _ => None,
            },
        }
    });
    if let Some(auth) = auth {
        return matches!(
            auth,
            AuthError::MissingCredential | AuthError::CredentialRejected { retryable: false }
        );
    }
    if matches!(
        error.downcast_ref::<crate::client::ClientError>(),
        Some(
            crate::client::ClientError::BackendMismatch { .. }
                | crate::client::ClientError::ClientVersionTooOld { .. }
        )
    ) {
        return true;
    }
    // Transparent variants and boxed errors can omit their inner source.
    if let Some(error) = error.downcast_ref::<Box<AuthError>>() {
        return terminal_source(error.as_ref());
    }
    if let Some(error) = error.downcast_ref::<Box<ApiClientError>>() {
        return terminal_source(error.as_ref());
    }
    match error.downcast_ref::<ApiClientError>() {
        Some(ApiClientError::Other(inner)) => terminal_source(inner.as_ref()),
        Some(ApiClientError::OtherUnretryable(inner)) => terminal_source(inner.as_ref()),
        _ => error.source().is_some_and(terminal_source),
    }
}

/// These replies prohibit an unchanged request.
pub(crate) fn rejected_request(error: &(dyn std::error::Error + 'static)) -> bool {
    if let Some(status) = error.downcast_ref::<tonic::Status>() {
        return matches!(
            status.code(),
            tonic::Code::InvalidArgument | tonic::Code::OutOfRange | tonic::Code::Unimplemented
        );
    }
    error.source().is_some_and(rejected_request)
}

/// Monotonic failure count and continuous accepted-registration interval.
/// A socket open alone does not start the healthy interval.
#[derive(Clone, Debug, Default)]
pub(crate) struct RecoverySnapshot {
    pub(crate) terminal: Option<RecoveryFailure>,
    pub(crate) idle: bool,
    pub(crate) failures: u64,
    pub(crate) healthy_since: Option<Instant>,
    pub(crate) outage_since: Option<Instant>,
    pub(crate) error: Option<Arc<IncomingError>>,
}

#[derive(Clone, Debug)]
pub(crate) enum RecoveryFailure {
    Exhausted {
        attempts: u32,
        source: Option<Arc<IncomingError>>,
    },
    Terminal(Arc<IncomingError>),
}

impl RecoveryFailure {
    pub(crate) fn error(&self) -> LocalDeliveryError {
        match self {
            Self::Exhausted { attempts, source } => LocalDeliveryError::NetworkRecoveryExhausted {
                attempts: *attempts,
                source: source.clone(),
            },
            Self::Terminal(error) => LocalDeliveryError::NetworkFailure(error.clone()),
        }
    }
}

/// The controller and delivery boundary check the same stream budget.
#[derive(Default)]
pub(crate) struct RecoveryState {
    pub(crate) snapshot: RecoverySnapshot,
    pub(crate) query_failures: u64,
    pub(crate) query_error: Option<(Topic, Arc<IncomingError>)>,
    pub(crate) last_transport_failures: u64,
    budget: Option<RecoveryBudget>,
}

impl RecoveryState {
    pub(crate) fn is_bounded(&self) -> bool {
        self.budget.is_some()
    }

    pub(crate) fn new(snapshot: RecoverySnapshot, bounded: bool, now: Instant) -> Self {
        let budget = bounded.then(|| RecoveryBudget::new(&snapshot, now));
        let last_transport_failures = snapshot.failures;
        Self {
            snapshot,
            query_failures: 0,
            query_error: None,
            last_transport_failures,
            budget,
        }
    }

    pub(crate) fn record_query_failure(
        &mut self,
        topic: &Topic,
        error: Arc<IncomingError>,
        transport_failures: u64,
        now: Instant,
    ) {
        if self.budget.is_none() || self.snapshot.terminal.is_some() {
            return;
        }
        self.query_failures = self.query_failures.saturating_add(1);
        self.query_error = Some((topic.clone(), error.clone()));
        self.snapshot.failures = transport_failures.saturating_add(self.query_failures);
        self.snapshot.error = Some(error);
        self.snapshot.healthy_since = None;
        self.snapshot.outage_since.get_or_insert(now);
        let _ = self.check(now);
    }

    pub(crate) fn clear_query_error(&mut self, topic: &Topic) {
        if self
            .query_error
            .as_ref()
            .is_some_and(|(failed, _)| failed == topic)
        {
            self.query_error = None;
        }
    }

    pub(crate) fn check(&mut self, now: Instant) -> Result<(), RecoveryFailure> {
        if let Some(failure) = &self.snapshot.terminal {
            return Err(failure.clone());
        }
        if let Some(budget) = &mut self.budget
            && let Err(failure) = budget.check(&self.snapshot, now)
        {
            self.snapshot.terminal = Some(failure.clone());
            return Err(failure);
        }
        Ok(())
    }
}

pub(crate) struct RecoveryBudget {
    baseline: u64,
    opened_at: Instant,
    outage_since: Option<Instant>,
}

impl RecoveryBudget {
    pub(crate) fn new(snapshot: &RecoverySnapshot, now: Instant) -> Self {
        Self {
            baseline: snapshot.failures,
            opened_at: now,
            outage_since: Some(now),
        }
    }

    pub(crate) fn check(
        &mut self,
        snapshot: &RecoverySnapshot,
        now: Instant,
    ) -> Result<(), RecoveryFailure> {
        if let Some(failure) = &snapshot.terminal {
            return Err(failure.clone());
        }
        if snapshot.idle {
            self.baseline = snapshot.failures;
            self.outage_since = None;
            return Ok(());
        }
        let failures = snapshot.failures.saturating_sub(self.baseline);
        if failures > 0
            && let Some(error) = &snapshot.error
            && terminal_source(error.as_ref())
        {
            tracing::warn!(
                code = error.code(),
                "stream network recovery stopped after a terminal error"
            );
            return Err(RecoveryFailure::Terminal(error.clone()));
        }
        // The transport keepalive detects a silent wire. All registrations must
        // remain accepted for this interval; application messages are not required.
        if snapshot
            .healthy_since
            .is_some_and(|since| now.saturating_duration_since(since) >= HEALTHY_PERIOD)
        {
            self.baseline = snapshot.failures;
            self.outage_since = None;
            return Ok(());
        }
        let started = self
            .outage_since
            .get_or_insert_with(|| snapshot.outage_since.unwrap_or(now).max(self.opened_at));
        if failures >= MAX_FAILURES || now.saturating_duration_since(*started) >= MAX_OUTAGE {
            tracing::warn!(
                attempts = failures,
                elapsed_ms = now.saturating_duration_since(*started).as_millis() as u64,
                cause = snapshot.error.as_ref().map(|error| error.code()),
                "stream network recovery budget exhausted"
            );
            return Err(RecoveryFailure::Exhausted {
                attempts: failures.min(u32::MAX as u64) as u32,
                source: snapshot.error.clone(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // verifies: PROC-021, PROC-038
    #[xmtp_common::test(unwrap_try = true)]
    fn a_nonretryable_response_uses_the_bounded_recovery_episode() {
        use xmtp_common::RetryableError;
        let now = Instant::now();
        let mut snapshot = RecoverySnapshot::default();
        let mut budget = RecoveryBudget::new(&snapshot, now);
        let error = Arc::new(IncomingError::Transport(
            xmtp_proto::api::NetworkError::new(xmtp_api::ApiError::InvalidResponse("cursor order")),
        ));
        assert!(!error.is_retryable());
        snapshot.error = Some(error.clone());
        for failures in 1..MAX_FAILURES {
            snapshot.failures = failures;
            budget.check(&snapshot, now)?;
        }
        snapshot.failures = MAX_FAILURES;
        let RecoveryFailure::Exhausted { attempts, source } =
            budget.check(&snapshot, now).unwrap_err()
        else {
            panic!("a response error must exhaust the budget, not terminate it immediately")
        };
        assert_eq!(attempts, MAX_FAILURES as u32);
        assert!(Arc::ptr_eq(&source.unwrap(), &error));
    }

    // verifies: PROC-021, AUTH-022
    #[xmtp_common::test(unwrap_try = true)]
    fn remote_cancellation_and_scope_refusal_end_the_stream_with_the_original_cause() {
        use xmtp_proto::api::{ApiClientError, NetworkError};
        for code in [tonic::Code::Cancelled, tonic::Code::PermissionDenied] {
            for message in ["", "Timeout expired", "connection closed"] {
                let now = Instant::now();
                let mut snapshot = RecoverySnapshot::default();
                let mut budget = RecoveryBudget::new(&snapshot, now);
                let cause = Arc::new(IncomingError::Transport(NetworkError::new(
                    ApiClientError::client(xmtp_api_grpc::error::GrpcError::Status(
                        tonic::Status::new(code, message),
                    )),
                )));
                snapshot.failures = 1;
                snapshot.error = Some(cause.clone());
                let RecoveryFailure::Terminal(error) = budget.check(&snapshot, now).unwrap_err()
                else {
                    panic!("explicit remote refusal must end the stream")
                };
                assert!(Arc::ptr_eq(&error, &cause));
            }
        }
        let mut status = tonic::Status::cancelled("Timeout expired");
        status.set_source(Arc::new(std::io::Error::other("connection closed")));
        assert!(terminal_source(&status));
        // Other permanent response codes still use the bounded recovery episode.
        for code in [tonic::Code::DataLoss, tonic::Code::Unauthenticated] {
            assert!(!terminal_source(&tonic::Status::new(code, "")));
        }
        for code in [
            tonic::Code::InvalidArgument,
            tonic::Code::OutOfRange,
            tonic::Code::Unimplemented,
        ] {
            assert!(terminal_source(&tonic::Status::new(code, "")));
            let query = IncomingError::Store(crate::mls_store::MlsStoreError::Api(
                xmtp_api::ApiError::Api(xmtp_proto::api::NetworkError::new(
                    xmtp_api_grpc::error::GrpcError::Status(tonic::Status::new(code, "")),
                )),
            ));
            assert!(rejected_request(&query));
            assert!(terminal_source(&query));
            #[cfg(not(target_arch = "wasm32"))]
            {
                let subscribe = IncomingError::Transport(xmtp_proto::api::NetworkError::new(
                    xmtp_api_backend::TransportError::Open(xmtp_api_backend::OpenError::new(
                        ApiClientError::client(xmtp_api_grpc::error::GrpcError::Status(
                            tonic::Status::new(code, ""),
                        )),
                    )),
                ));
                assert!(terminal_source(&subscribe));
                assert!(rejected_request(&subscribe));
            }
        }
    }

    xmtp_common::if_native! {
    #[xmtp_common::test(unwrap_try = true)]
    fn a_typed_local_timeout_does_not_end_application_recovery() {
        use xmtp_proto::api::{ApiClientError, NetworkError};
        let now = Instant::now();
        let mut snapshot = RecoverySnapshot::default();
        let mut budget = RecoveryBudget::new(&snapshot, now);
        let status = tonic::Status::from_error(Box::new(tonic::TimeoutExpired(())));
        assert_eq!(status.code(), tonic::Code::Cancelled);
        snapshot.failures = 1;
        snapshot.error = Some(Arc::new(IncomingError::Transport(NetworkError::new(
            ApiClientError::client(xmtp_api_grpc::error::GrpcError::Status(status)),
        ))));
        budget.check(&snapshot, now)?;
    }
    }

    // verifies: AUTH-025, CONF-022
    #[xmtp_common::test(unwrap_try = true)]
    fn only_credentials_requiring_app_action_and_configuration_latches_are_terminal() {
        use xmtp_proto::api::{ApiClientError, AuthError, NetworkError};
        for (auth, terminal) in [
            (AuthError::MissingCredential, true),
            (AuthError::CredentialRejected { retryable: false }, true),
            (AuthError::CredentialRejected { retryable: true }, false),
            (AuthError::CallbackFailed { retryable: true }, false),
            (AuthError::CallbackFailed { retryable: false }, false),
            (AuthError::Exhausted, false),
            (AuthError::ExhaustedAfterAttempt, false),
        ] {
            for error in [
                NetworkError::new(auth),
                NetworkError::new(ApiClientError::Auth(auth)),
                NetworkError::new(xmtp_api::ApiError::Auth(auth)),
                NetworkError::new(Box::new(auth)),
                NetworkError::new(Box::new(ApiClientError::Auth(auth))),
                NetworkError::new(ApiClientError::other(auth)),
            ] {
                assert_eq!(terminal_source(&IncomingError::Transport(error)), terminal);
            }
        }
        for error in [
            crate::client::ClientError::BackendMismatch {
                stored: "first".into(),
                received: "second".into(),
            },
            crate::client::ClientError::ClientVersionTooOld {
                client: "1.0.0".into(),
                minimum: "2.0.0".into(),
            },
        ] {
            assert!(terminal_source(&IncomingError::Transport(
                NetworkError::new(error)
            )));
        }
    }

    // verifies: PROC-038, AUTH-025
    #[xmtp_common::test(unwrap_try = true)]
    fn callback_failure_and_lockout_recover_without_resetting_the_episode_early() {
        use xmtp_proto::api::{AuthError, NetworkError};
        let now = Instant::now();
        let mut snapshot = RecoverySnapshot::default();
        let mut budget = RecoveryBudget::new(&snapshot, now);
        snapshot.failures = 1;
        snapshot.error = Some(Arc::new(IncomingError::Transport(NetworkError::new(
            AuthError::CallbackFailed { retryable: true },
        ))));
        budget.check(&snapshot, now)?;
        snapshot.failures = 2;
        snapshot.error = Some(Arc::new(IncomingError::Transport(NetworkError::new(
            AuthError::Exhausted,
        ))));
        budget.check(&snapshot, now + Duration::from_secs(1))?;
        // Waiting adds elapsed time, not another failed recovery cycle.
        budget.check(&snapshot, now + Duration::from_secs(59))?;
        assert_eq!(snapshot.failures, 2);
        snapshot.healthy_since = Some(now + Duration::from_secs(60));
        budget.check(&snapshot, now + Duration::from_secs(60) + HEALTHY_PERIOD)?;
        snapshot.healthy_since = None;
        snapshot.failures += MAX_FAILURES - 1;
        snapshot.outage_since = Some(now + MAX_OUTAGE * 2);
        budget.check(&snapshot, now + MAX_OUTAGE * 2)?;
    }

    // verifies: PROC-038, PROC-039, AUTH-025
    #[xmtp_common::test(unwrap_try = true)]
    fn lockout_wait_keeps_the_deadline_and_a_replacement_gets_a_fresh_budget() {
        use xmtp_proto::api::{AuthError, NetworkError};
        let now = Instant::now();
        let mut snapshot = RecoverySnapshot::default();
        let mut budget = RecoveryBudget::new(&snapshot, now);
        snapshot.failures = 1;
        snapshot.error = Some(Arc::new(IncomingError::Transport(NetworkError::new(
            AuthError::CallbackFailed { retryable: true },
        ))));
        budget.check(&snapshot, now)?;
        snapshot.failures = 2;
        let cause = Arc::new(IncomingError::Transport(NetworkError::new(
            AuthError::Exhausted,
        )));
        snapshot.error = Some(cause.clone());
        budget.check(&snapshot, now + MAX_OUTAGE - Duration::from_millis(1))?;
        let RecoveryFailure::Exhausted { attempts, source } =
            budget.check(&snapshot, now + MAX_OUTAGE).unwrap_err()
        else {
            panic!("lockout must use the deadline, not immediate termination")
        };
        assert_eq!(attempts, 2);
        assert!(Arc::ptr_eq(&source.unwrap(), &cause));
        let mut replacement = RecoveryBudget::new(&snapshot, now + MAX_OUTAGE);
        replacement.check(&snapshot, now + MAX_OUTAGE)?;
    }

    // verifies: PROC-038
    #[xmtp_common::test(unwrap_try = true)]
    fn one_stream_survives_twelve_separate_outages() {
        let start = Instant::now();
        let mut now = start;
        let mut snapshot = RecoverySnapshot::default();
        let mut budget = RecoveryBudget::new(&snapshot, now);
        for _ in 0..12 {
            snapshot.failures += MAX_FAILURES - 1;
            snapshot.healthy_since = None;
            budget.check(&snapshot, now)?;
            snapshot.healthy_since = Some(now);
            now += HEALTHY_PERIOD;
            budget.check(&snapshot, now)?;
        }
        assert!(snapshot.failures > MAX_FAILURES);
    }

    // verifies: PROC-038
    #[xmtp_common::test(unwrap_try = true)]
    fn a_paused_consumer_keeps_the_observed_outage_deadline() {
        let now = Instant::now();
        let mut snapshot = RecoverySnapshot {
            healthy_since: Some(now),
            ..Default::default()
        };
        let mut budget = RecoveryBudget::new(&snapshot, now);
        budget.check(&snapshot, now + HEALTHY_PERIOD)?;
        snapshot.healthy_since = None;
        snapshot.outage_since = Some(now + HEALTHY_PERIOD);
        snapshot.failures = 1;
        assert!(
            budget
                .check(&snapshot, now + HEALTHY_PERIOD + MAX_OUTAGE)
                .is_err()
        );
    }

    // verifies: PROC-038
    #[xmtp_common::test(unwrap_try = true)]
    fn healthy_reset_survives_a_pause_that_also_spans_the_next_outage() {
        let now = Instant::now();
        let mut state = RecoveryState::new(RecoverySnapshot::default(), true, now);
        state.snapshot.failures = MAX_FAILURES - 1;
        state.check(now)?;
        // The controller checks this same record while the app holds an item.
        state.snapshot.healthy_since = Some(now);
        state.check(now + HEALTHY_PERIOD)?;
        state.snapshot.healthy_since = None;
        state.snapshot.failures += 1;
        state.snapshot.outage_since = Some(now + MAX_OUTAGE * 2);
        state.check(now + MAX_OUTAGE * 2 + Duration::from_secs(1))?;
        state.snapshot.failures += MAX_FAILURES - 1;
        assert!(
            state
                .check(now + MAX_OUTAGE * 2 + Duration::from_secs(2))
                .is_err()
        );
    }

    // verifies: PROC-038, PROC-039
    #[xmtp_common::test(unwrap_try = true)]
    fn an_exhausted_episode_stays_terminal_after_health_returns() {
        let now = Instant::now();
        let mut state = RecoveryState::new(RecoverySnapshot::default(), true, now);
        state.snapshot.failures = MAX_FAILURES;
        assert!(state.check(now).is_err());
        state.snapshot.healthy_since = Some(now);
        assert!(state.check(now + HEALTHY_PERIOD).is_err());
        // A new lease copies shared counters, not the old terminal outcome.
        let fresh = RecoverySnapshot {
            failures: MAX_FAILURES,
            healthy_since: Some(now),
            ..Default::default()
        };
        let mut replacement = RecoveryState::new(fresh, true, now + HEALTHY_PERIOD);
        replacement.check(now + HEALTHY_PERIOD)?;
    }

    // verifies: PROC-039
    #[xmtp_common::test(unwrap_try = true)]
    fn new_stream_has_fresh_budget_during_the_same_outage() {
        let now = Instant::now();
        let mut snapshot = RecoverySnapshot::default();
        let mut old = RecoveryBudget::new(&snapshot, now);
        snapshot.failures = MAX_FAILURES;
        assert!(old.check(&snapshot, now).is_err());
        let mut replacement = RecoveryBudget::new(&snapshot, now);
        replacement.check(&snapshot, now)?;
        snapshot.failures += MAX_FAILURES - 1;
        replacement.check(&snapshot, now)?;
        snapshot.failures += 1;
        assert!(replacement.check(&snapshot, now).is_err());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn hung_open_expires_without_an_attempt_event() {
        let now = Instant::now();
        let snapshot = RecoverySnapshot::default();
        let mut budget = RecoveryBudget::new(&snapshot, now);
        budget.check(&snapshot, now + MAX_OUTAGE - Duration::from_millis(1))?;
        assert!(budget.check(&snapshot, now + MAX_OUTAGE).is_err());
    }

    // verifies: PROC-038
    #[xmtp_common::test(unwrap_try = true)]
    fn empty_interest_does_not_expire_during_an_unrelated_outage() {
        let now = Instant::now();
        let snapshot = RecoverySnapshot {
            idle: true,
            ..Default::default()
        };
        let mut budget = RecoveryBudget::new(&snapshot, now);
        budget.check(&snapshot, now + MAX_OUTAGE * 2)?;
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn flapping_does_not_reset_but_quiet_registered_connection_does() {
        let now = Instant::now();
        let mut snapshot = RecoverySnapshot::default();
        let mut budget = RecoveryBudget::new(&snapshot, now);
        for failures in 1..MAX_FAILURES {
            snapshot.failures = failures;
            snapshot.healthy_since = Some(now);
            budget.check(&snapshot, now + HEALTHY_PERIOD - Duration::from_millis(1))?;
        }
        budget.check(&snapshot, now + HEALTHY_PERIOD)?;
        snapshot.healthy_since = None;
        snapshot.failures += MAX_FAILURES - 1;
        budget.check(&snapshot, now + HEALTHY_PERIOD)?;
        snapshot.failures += 1;
        assert!(budget.check(&snapshot, now + HEALTHY_PERIOD).is_err());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn consumers_do_not_share_an_outage_deadline() {
        let now = Instant::now();
        let snapshot = RecoverySnapshot::default();
        let mut first = RecoveryBudget::new(&snapshot, now);
        let mut second = RecoveryBudget::new(&snapshot, now + Duration::from_secs(60));
        assert!(first.check(&snapshot, now + MAX_OUTAGE).is_err());
        second.check(&snapshot, now + MAX_OUTAGE)?;
    }
}
