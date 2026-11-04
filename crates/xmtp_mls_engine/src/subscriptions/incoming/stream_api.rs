//! The receiver operations that application streams use.
//!
//! A stream acquires its lease with [`IncomingCoordinator::acquire_stream`],
//! which gives that lease its own recovery budget. While it waits, the stream
//! rechecks the budget with [`IncomingLease::check_recovery`] on a timer as
//! well as on lease changes, and ends with the [`RecoveryFailure`] it returns.
//! A stream reports its last status with [`IncomingStatus::cancel`] applied
//! when it closes, and a conversation stream skips rediscovery with a
//! [`GroupSetGate`]. The order and timing of these checks belong to each stream.
//!
//! On native targets, the `shared_wires` child module owns the shared Bidi
//! transports and their suspension, which the app's suspend and resume calls
//! drive.

use super::{
    IncomingConnection, IncomingCoordinator, IncomingError, IncomingLease, IncomingProcessing,
    IncomingRegistration, IncomingScope, IncomingStatus,
};
use std::sync::Arc;
use xmtp_common::time::{Duration, Instant};

#[cfg(not(target_arch = "wasm32"))]
pub mod shared_wires;

/// How often a waiting message reader rechecks its recovery budget without
/// another wake, so a budget that runs out by time alone still ends the stream.
pub const RECOVERY_POLL: Duration = Duration::from_secs(1);

/// Why a stream's recovery budget ended the stream.
#[derive(Clone, Debug)]
pub enum RecoveryFailure {
    /// The stream failed too often, or stayed disconnected too long, to retry.
    Exhausted {
        /// Failed attempts counted against this stream's budget.
        attempts: u32,
        /// The latest failure, or `None` when the deadline passed without one.
        source: Option<Arc<IncomingError>>,
    },
    /// A failure that retrying cannot fix ended the stream at once.
    Terminal(Arc<IncomingError>),
}

/// The group-set generation (`QueryGroup::group_set_generation`) of an
/// operation's last successful discovery read, so a rediscovery is skipped
/// while the stored group set is unchanged.
///
/// Read the generation before the discovery query and record it only after
/// the query succeeds. A write that commits between the two reads then changes
/// the generation the next pass sees, so the next pass reads again. Reading the
/// generation after the query could cache an old set under a new generation.
#[derive(Debug, Default)]
pub struct GroupSetGate(Option<i64>);

impl GroupSetGate {
    /// Whether `current` differs from the recorded generation, or none is
    /// recorded, so the operation must read again.
    pub fn changed(&self, current: i64) -> bool {
        self.0 != Some(current)
    }

    /// Record the generation read before a discovery read that succeeded.
    pub fn record(&mut self, current: i64) {
        self.0 = Some(current);
    }
}

impl IncomingCoordinator {
    /// Keep this scope active for an application stream until the returned
    /// lease closes or drops.
    ///
    /// Application streams own a finite outage budget. Internal workers and
    /// bounded sync operations keep their existing lifetime and deadline rules.
    pub fn acquire_stream(self: &Arc<Self>, scope: IncomingScope) -> IncomingLease {
        self.acquire_with_recovery(scope, true)
    }
}

impl IncomingLease {
    /// The failure that ended this lease's recovery budget, if one did.
    ///
    /// A lease from [`IncomingCoordinator::acquire`] has no budget, and a
    /// closed lease no longer reports one, so both return `Ok`.
    pub fn check_recovery(&self) -> Result<(), RecoveryFailure> {
        let mut consumers = self.coordinator.state.consumer_recovery.lock();
        match consumers.get_mut(&self.id) {
            Some(state) => state.check(Instant::now()),
            None => Ok(()),
        }
    }
}

impl IncomingStatus {
    /// Report this status as ended: the connection closed, processing and
    /// every topic cancelled, every topic registration removed, and no
    /// discovery pending.
    pub fn cancel(&mut self) {
        self.connection = IncomingConnection::Closed;
        self.processing = IncomingProcessing::Cancelled;
        self.discovery_pending = false;
        for topic in &mut self.topics {
            topic.registration = IncomingRegistration::Removed;
            topic.processing = IncomingProcessing::Cancelled;
        }
    }
}
