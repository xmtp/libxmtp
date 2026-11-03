//! Epoch faults that tests inject into message processing.

use crate::groups::mls_sync::GroupMessageProcessingError;
use crate::groups::mls_sync::GroupMessageProcessingError::OpenMlsProcessMessage;
use openmls::group::ProcessMessageError;
use openmls::group::ValidationError::WrongEpoch;
use std::sync::atomic::{AtomicBool, Ordering};

static FUTURE_WRONG_EPOCH: AtomicBool = AtomicBool::new(false);

/* ---------- wrong / future epoch helpers ---------- */

/// Toggle wrong‑epoch test‑mode.
#[inline]
pub fn set_test_mode_future_wrong_epoch(enable: bool) {
    FUTURE_WRONG_EPOCH.store(enable, Ordering::Relaxed);
}

#[inline]
pub fn is_test_mode_future_wrong_epoch() -> bool {
    FUTURE_WRONG_EPOCH.load(Ordering::Relaxed)
}

/// If the flag is set, fail with a *wrong epoch* validation error.
pub fn maybe_mock_wrong_epoch_for_tests() -> Result<(), GroupMessageProcessingError> {
    if is_test_mode_future_wrong_epoch() {
        return Err(OpenMlsProcessMessage(ProcessMessageError::ValidationError(
            WrongEpoch,
        )));
    }
    Ok(())
}

/// If the flag is set, fail with a *future epoch* error.
pub fn maybe_mock_future_epoch_for_tests() -> Result<(), GroupMessageProcessingError> {
    if is_test_mode_future_wrong_epoch() {
        return Err(GroupMessageProcessingError::FutureEpoch(10, 0));
    }
    Ok(())
}
