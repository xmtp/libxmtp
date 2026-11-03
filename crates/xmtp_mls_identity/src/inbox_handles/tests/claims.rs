//! The claim cap, checked before a claim is signed.

use super::new_inbox_state;
use crate::inbox_handles::{HandleError, ensure_claim_allowed};
use xmtp_id::associations::{MAX_HANDLE_CLAIMS, handle::Handle};

#[xmtp_common::test(unwrap_try = true)]
fn claim_limit_is_checked_before_signing() {
    let handle = Handle::parse("nick")?;
    let mut state = new_inbox_state();
    for _ in 1..MAX_HANDLE_CLAIMS {
        state = state.claim_handle(handle.clone());
    }
    assert_eq!(state.handle_claims(), MAX_HANDLE_CLAIMS - 1);
    assert!(ensure_claim_allowed(&state).is_ok());

    let state = state.claim_handle(handle);
    assert!(matches!(
        ensure_claim_allowed(&state),
        Err(HandleError::LimitReached)
    ));
}
