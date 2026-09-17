//! Typed read facade over an OpenMLS group's AppData dictionary.
//!
//! [`MlsGroupAppData`] borrows a group's `GroupContext` extensions and
//! exposes typed, per-component reads via [`MlsGroupAppData::get`]. Write paths
//! continue to use the existing intent infrastructure (`mls_sync.rs`)
//! plus `stage_app_data_propose_and_commit`; this facade is for callers
//! that need a single typed value (e.g. permissions checks, registry
//! lookups, custom-component reads).
//!

use openmls::extensions::Extensions;
use openmls::group::GroupContext;
use xmtp_mls_common::app_data::typed::Component;

use super::component_source::{ComponentSourceError, read_component_bytes};

/// A typed view over a group's AppData state.
///
/// Holds a borrow of the group's `GroupContext` extensions. All AppData lives
/// in those extensions, so this deliberately does **not** need a full `OpenMlsGroup` (which would
/// pull the ratchet tree and secrets) — a `StorageProvider::group_context`
/// read is enough. Cheap to construct; discarded after use.
pub(crate) struct MlsGroupAppData<'g> {
    extensions: &'g Extensions<GroupContext>,
}

impl<'g> MlsGroupAppData<'g> {
    /// Wrap a group's `GroupContext` extensions for typed AppData reads.
    ///
    pub(crate) fn new(extensions: &'g Extensions<GroupContext>) -> Self {
        Self { extensions }
    }

    /// Read the typed value of a [`Component`] from this group.
    ///
    /// Returns `Ok(None)` if the component has no current bytes.
    /// Returns `Ok(Some(value))` on a successful decode of bytes.
    /// Returns `Err` for transport-level (read) or codec-level
    /// (decode) failures.
    pub(crate) fn get<C: Component>(&self) -> Result<Option<C::Value>, ComponentSourceError> {
        let bytes = read_component_bytes(C::ID, self.extensions)?;
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
