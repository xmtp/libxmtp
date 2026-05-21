//! Condition bits for healthcheck ops/validators. An op declares
//! which conditions it needs via its registration metadata; the
//! runner builds the active set from runtime flags. Ops whose
//! `requires` isn't fully covered by active conditions are skipped
//! (recorded, not failed).

use bitflags::bitflags;

bitflags! {
    /// Condition bits. Start with a single axis; widen the backing
    /// type if we cross ~6 axes.
    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
    pub struct Conditions: u8 {
        /// `--strict-versioning` is in effect. Required by ops whose
        /// semantics depend on the same-version vs other-version
        /// partition being meaningful (e.g. per-version membership
        /// assertions). Without strict, `existing_clients` conflates
        /// versions and those assertions are vacuous.
        const STRICT_VERSIONING = 1 << 0;
        /// Mutating ops (Create/Add/Update/Remove/Leave) require this
        /// bit. Default-on; cleared by `--read-only`.
        const WRITES = 1 << 1;
    }
}

impl Conditions {
    /// "No required conditions — this op runs under any active set."
    /// Use this in `inventory::submit!` blocks for ungated ops.
    /// Identical to `Conditions::empty()` but reads as intent:
    /// "this op is unconditional," not "this op needs an empty set."
    pub const ALWAYS: Conditions = Conditions::empty();

    /// Read the active condition set from runtime sources. Currently
    /// just `App::strict_versioning()`; extend as new axes are added.
    pub fn active() -> Self {
        let mut c = Conditions::empty();
        if crate::app::App::strict_versioning() {
            c |= Conditions::STRICT_VERSIONING;
        }
        c
    }

    /// Condition bits an entry requires that aren't present in
    /// `active`. Empty result means the entry is runnable. Thin
    /// wrapper over `bitflags::difference` for readability at call
    /// sites.
    pub fn missing_from(self, active: Conditions) -> Conditions {
        self.difference(active)
    }
}
