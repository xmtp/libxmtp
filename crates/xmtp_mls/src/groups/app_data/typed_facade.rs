//! Typed read facade over an OpenMLS group's AppData dictionary.
//!
//! [`MlsGroupAppData`] borrows an `&OpenMlsGroup` and exposes typed,
//! per-component reads via [`MlsGroupAppData::get`]. Write paths
//! continue to use the existing intent infrastructure (`mls_sync.rs`)
//! plus `stage_inline_app_data_commit`; this facade is for callers
//! that need a single typed value (e.g. permissions checks, registry
//! lookups, custom-component reads).
//!
//! ## Capability awareness
//!
//! On unmigrated groups the dict is empty, so [`MlsGroupAppData::get`]
//! falls back to the legacy group-context-extension translation via
//! [`super::component_source::read_component_bytes`]. On migrated
//! groups (post-bootstrap) the dict is authoritative. Either way the
//! caller gets `C::Value` decoded — no manual capability switching.

use openmls::group::MlsGroup as OpenMlsGroup;
use xmtp_mls_common::app_data::typed::Component;

use super::component_source::{ComponentSourceError, read_component_bytes};
use super::is_migrated_group;

/// A typed view over a group's AppData state.
///
/// Holds nothing but the borrow + the migration flag. Cheap to
/// construct; intended to be created locally inside a
/// `load_mls_group_with_lock` closure and discarded.
pub(crate) struct MlsGroupAppData<'g> {
    group: &'g OpenMlsGroup,
    proposals_enabled: bool,
}

impl<'g> MlsGroupAppData<'g> {
    /// Wrap an `&OpenMlsGroup` for typed AppData reads.
    ///
    /// **Construct under the same `load_mls_group_with_lock` closure
    /// that consumes the facade.** The cached `proposals_enabled`
    /// flag is read once at construction, so a facade that outlives
    /// the lock could observe a stale migration state on a subsequent
    /// `get` call. In practice every call site lives inside one
    /// closure and discards the facade at the end.
    pub(crate) fn new(group: &'g OpenMlsGroup) -> Self {
        let proposals_enabled = is_migrated_group(group);
        Self {
            group,
            proposals_enabled,
        }
    }

    /// Read the typed value of a [`Component`] from this group.
    ///
    /// Returns `Ok(None)` if the component has no current bytes (slot
    /// missing in the dict, or the legacy extension wasn't populated).
    /// Returns `Ok(Some(value))` on a successful decode of bytes.
    /// Returns `Err` for transport-level (read) or codec-level
    /// (decode) failures.
    pub(crate) fn get<C: Component>(&self) -> Result<Option<C::Value>, ComponentSourceError> {
        let bytes = read_component_bytes(C::ID, self.group, self.proposals_enabled)?;
        match bytes {
            Some(b) => Ok(Some(C::decode_value(&b)?)),
            None => Ok(None),
        }
    }
}

// End-to-end coverage lives in the bootstrap-flow integration tests
// in `tests/test_proposals.rs`; constructing an `OpenMlsGroup`
// outside the full keystore setup is expensive and adds little
// signal beyond what `read_component_bytes` and
// `Component::decode_value` already pin in their own modules.
