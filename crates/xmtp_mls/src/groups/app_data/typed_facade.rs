//! Typed read facade over an OpenMLS group's AppData dictionary.
//!
//! [`MlsGroupAppData`] borrows a group's `GroupContext` extensions and
//! exposes typed, per-component reads via [`MlsGroupAppData::get`]. Write paths
//! continue to use the existing intent infrastructure (`mls_sync.rs`)
//! plus `stage_app_data_propose_and_commit`; this facade is for callers
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

use openmls::extensions::Extensions;
use openmls::group::GroupContext;
use xmtp_mls_common::app_data::typed::Component;

use super::component_source::{ComponentSourceError, read_component_bytes};
use super::is_migrated_extensions;

/// A typed view over a group's AppData state.
///
/// Holds nothing but a borrow of the group's `GroupContext` extensions
/// plus the migration flag. All AppData lives in those extensions, so
/// this deliberately does **not** need a full `OpenMlsGroup` (which would
/// pull the ratchet tree and secrets) — a `StorageProvider::group_context`
/// read is enough. Cheap to construct; discarded after use.
pub(crate) struct MlsGroupAppData<'g> {
    extensions: &'g Extensions<GroupContext>,
    proposals_enabled: bool,
}

impl<'g> MlsGroupAppData<'g> {
    /// Wrap a group's `GroupContext` extensions for typed AppData reads.
    ///
    /// The cached `proposals_enabled` flag is read once at construction
    /// from the same extensions snapshot, so every `get` observes a
    /// single consistent view.
    pub(crate) fn new(extensions: &'g Extensions<GroupContext>) -> Self {
        let proposals_enabled = is_migrated_extensions(extensions);
        Self {
            extensions,
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
        let bytes = read_component_bytes(C::ID, self.extensions, self.proposals_enabled)?;
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
