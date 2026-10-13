//! Handle policy that only the backend applies, and only when it admits a
//! claim. No validator applies it to a stored log, so a change here never
//! stops a stored log from replaying, and a handle claimed before the change
//! stays valid.

use crate::api::publish_error::Reason;
use xmtp_common::NS_IN_DAY;
use xmtp_id::associations::Handle;
use xmtp_proto::xmtp::identity::associations::{IdentityUpdate, identity_action};

#[cfg(test)]
mod tests;

/// Name parts no inbox may claim, with or without a code. The list is not
/// published, and a later release may change it.
pub(crate) const RESERVED_NAMES: [&str; 17] = [
    "admin",
    "administrator",
    "support",
    "help",
    "xmtp",
    "root",
    "system",
    "official",
    "staff",
    "moderator",
    "security",
    "abuse",
    "postmaster",
    "hostmaster",
    "webmaster",
    "noreply",
    "everyone",
];

/// The shortest time between two claims by one inbox, in nanoseconds: 7 days.
pub(crate) const CLAIM_INTERVAL_NS: i64 = 7 * NS_IN_DAY;

/// Why the backend refused a claim that every validator accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleRejection {
    /// Another inbox holds the handle.
    Taken,
    /// The name part is reserved or shorter than the configured minimum.
    NotAllowed,
    /// The inbox's previous claim is less than [`CLAIM_INTERVAL_NS`] old.
    TooSoon,
}

impl HandleRejection {
    /// The publish reason the rejection reports.
    pub fn reason(self) -> Reason {
        match self {
            Self::Taken => Reason::HandleTaken,
            Self::NotAllowed => Reason::HandleNotAllowed,
            Self::TooSoon => Reason::HandleTooSoon,
        }
    }

    /// A fixed message, so no handle reaches a status or a log.
    pub fn message(self) -> &'static str {
        match self {
            Self::Taken => "handle is taken",
            Self::NotAllowed => "handle is not allowed",
            Self::TooSoon => "handle claimed too soon after the previous claim",
        }
    }
}

/// Whether a stored or new identity update carries a `ClaimHandle`.
pub(crate) fn carries_claim(update: &IdentityUpdate) -> bool {
    update
        .actions
        .iter()
        .any(|action| matches!(action.kind, Some(identity_action::Kind::ClaimHandle(_))))
}

/// The checks of a new claim that need no lock, in the order a rejection
/// reports them: the name first, then the time since the previous claim.
/// The taken check runs later, under the identity lock.
pub(crate) fn check_claim(
    handle: &Handle,
    min_name_length: u32,
    stored_claims_server_ns: impl IntoIterator<Item = i64>,
    clock_ns: i64,
) -> Result<(), HandleRejection> {
    check_policy(handle, min_name_length)?;
    check_interval(stored_claims_server_ns, clock_ns)
}

/// Refuse a name part that is reserved or shorter than `min_name_length`.
/// The check applies to a handle the inbox already holds, too.
// implements: API-297
pub(crate) fn check_policy(handle: &Handle, min_name_length: u32) -> Result<(), HandleRejection> {
    let name = handle.name();
    // The grammar is ASCII, so the byte length is the character count.
    let too_short = u32::try_from(name.len()).is_ok_and(|length| length < min_name_length);
    if too_short || RESERVED_NAMES.contains(&name) {
        return Err(HandleRejection::NotAllowed);
    }
    Ok(())
}

/// Refuse a claim when any stored claim of the inbox has a `server_ns` less
/// than [`CLAIM_INTERVAL_NS`] before `clock_ns`, the backend's clock at
/// admission. Client timestamps are never read.
// implements: API-298
pub(crate) fn check_interval(
    stored_claims_server_ns: impl IntoIterator<Item = i64>,
    clock_ns: i64,
) -> Result<(), HandleRejection> {
    let earliest_allowed = clock_ns.saturating_sub(CLAIM_INTERVAL_NS);
    if stored_claims_server_ns
        .into_iter()
        .any(|server_ns| server_ns > earliest_allowed)
    {
        return Err(HandleRejection::TooSoon);
    }
    Ok(())
}
