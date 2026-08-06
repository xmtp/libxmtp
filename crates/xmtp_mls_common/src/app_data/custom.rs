//! Host-defined ("custom") component registration.
//!
//! Custom components live in the app range (`0xC000-0xFEFF`,
//! per [`ComponentId::is_app_range`]) and are registered by host
//! apps at process startup. Unlike well-known components — which
//! have static `Component` impls with `const ID: ComponentId` — a
//! [`RuntimeComponent`] carries its id as a runtime value so a host
//! can register multiple distinct components from one impl type
//! parameterized by id (or build them dynamically).
//!
//! ## Lifecycle
//!
//! 1. **Process startup** (host): the host calls
//!    [`register_global_runtime_component`] for each component it
//!    wants to support. The registry stores a `&'static dyn
//!    ErasedComponent`-shaped handle keyed by `ComponentId`.
//! 2. **Dispatch lookup**: callers of
//!    [`super::registry_table::lookup_component`] check the static
//!    `WELL_KNOWN` table first; if no entry exists there and the id
//!    is in the app range, the runtime registry is consulted.
//! 3. **Per-group registration**: registering a custom component in
//!    a *group's* `COMPONENT_REGISTRY` happens via
//!    `IntentKind::UpdatePermission` (post-bootstrap), separately
//!    from the host-process registration here. Both must happen
//!    before a group can carry a custom component's value: the host
//!    needs a `RuntimeComponent` impl to decode the bytes, and the
//!    group needs a registry entry so writes pass
//!    `validate_component_write`.
//!
//! ## Why a global rather than per-Client
//!
//! Threading a per-`Client` registry through every dispatch site
//! (`apply_app_data_update_payload`, `expand_app_data_update_to_changes`,
//! `validate_component_write`, the facade) would touch ~dozens of
//! call shapes. A process-global registry keeps the dispatch
//! signature stable and reflects the operational reality: hosts
//! register their custom-component shapes once, at process startup,
//! before any group is created. There is no use case for "different
//! Clients in the same process know different custom components."

use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

use parking_lot::RwLock;

use openmls::messages::proposals::AppDataUpdateOperation;
use xmtp_common::{MaybeSend, MaybeSync};
use xmtp_proto::xmtp::mls::message_contents::ComponentType;

use super::{
    component_id::ComponentId,
    component_registry::ComponentRegistry,
    typed::{
        ComponentInvariantError, ComponentTypedError, ErasedComponent, ExpandedComponentChange,
    },
    validation::ComponentChange,
};

/// A host-defined component whose `ComponentId` is known at runtime.
///
/// Unlike [`super::typed::Component`], `RuntimeComponent` takes
/// `&self` and reports its id via [`Self::id`] so a single impl can
/// service multiple distinct ids (or build them dynamically). The
/// trait is otherwise structurally identical to `Component` minus
/// the typed `Value` / `Mutation` associated types — runtime
/// components handle bytes only, since the host owns whatever
/// further decoding it does on top.
///
/// The bounds are [`MaybeSend`] + [`MaybeSync`] rather than `Send +
/// Sync` so that WASM hosts can register impls that hold non-`Send`
/// state (e.g. JS-bound objects). On native targets these expand to
/// `Send + Sync` and the trait behaves identically to before; on
/// `wasm32` they expand to vacuous bounds. The dispatch path's static
/// storage requirements are bridged in [`RuntimeAdapter`] below — see
/// the SAFETY note there.
pub trait RuntimeComponent: MaybeSend + MaybeSync + 'static {
    /// The component this instance handles.
    fn id(&self) -> ComponentId;

    /// Logical wire type, written into the registry entry's
    /// `ComponentMetadata.component_type` slot. The host typically
    /// picks one of the existing [`ComponentType`] variants
    /// (`Bytes`, `String`, `TlsSetInboxId`, etc.) — runtime
    /// components are not currently expected to introduce new
    /// `ComponentType`s.
    fn component_type(&self) -> ComponentType;

    /// Apply an `AppDataUpdateOperation::Update` payload against the
    /// prior bytes (if any) and produce the new full-state bytes.
    fn apply_update_payload(
        &self,
        payload: &[u8],
        prior: Option<&[u8]>,
    ) -> Result<Vec<u8>, ComponentTypedError>;

    /// Expand an `AppDataUpdate` proposal into per-element changes
    /// for the validator's policy loop.
    fn expand_to_changes(
        &self,
        op: &AppDataUpdateOperation,
        prior: Option<&[u8]>,
    ) -> Result<Vec<ExpandedComponentChange>, ComponentTypedError>;

    /// Optional component-local invariant check. Default no-op.
    fn validate_invariant(
        &self,
        _change: &ComponentChange<'_>,
        _registry: &ComponentRegistry,
    ) -> Result<(), ComponentInvariantError> {
        Ok(())
    }
}

/// Adapter so a [`RuntimeComponent`] can be used wherever an
/// [`ErasedComponent`] is expected. The dispatch table consults
/// runtime components via this adapter.
struct RuntimeAdapter(Arc<dyn RuntimeComponent>);

// SAFETY: wasm32 is single-threaded; the runtime registry's
// `Arc<dyn RuntimeComponent>` is only ever accessed from the same
// thread that registered it. On native, `RuntimeComponent` carries
// `Send + Sync` (via `MaybeSend + MaybeSync`) so this impl is
// unnecessary — only WASM needs the manual bridge because the trait
// bounds are vacuous there but `ErasedComponent` (used by the static
// dispatch table) still requires `Send + Sync`.
#[cfg(target_arch = "wasm32")]
unsafe impl Send for RuntimeAdapter {}
#[cfg(target_arch = "wasm32")]
unsafe impl Sync for RuntimeAdapter {}

impl ErasedComponent for RuntimeAdapter {
    fn id(&self) -> ComponentId {
        self.0.id()
    }

    fn component_type(&self) -> ComponentType {
        self.0.component_type()
    }

    fn apply_update_payload(
        &self,
        payload: &[u8],
        prior: Option<&[u8]>,
    ) -> Result<Vec<u8>, ComponentTypedError> {
        self.0.apply_update_payload(payload, prior)
    }

    fn expand_to_changes(
        &self,
        op: &AppDataUpdateOperation,
        prior: Option<&[u8]>,
    ) -> Result<Vec<ExpandedComponentChange>, ComponentTypedError> {
        self.0.expand_to_changes(op, prior)
    }

    fn validate_invariant(
        &self,
        change: &ComponentChange<'_>,
        registry: &ComponentRegistry,
    ) -> Result<(), ComponentInvariantError> {
        self.0.validate_invariant(change, registry)
    }
}

/// Errors surfaced by [`register_global_runtime_component`].
#[derive(Debug, thiserror::Error)]
pub enum RuntimeRegistrationError {
    /// The id is outside the app range (`0xC000-0xFEFF`).
    /// XMTP-range ids are reserved for static well-known impls.
    #[error("component id {0} is not in the app-defined range (0xC000-0xFEFF)")]
    OutOfRange(ComponentId),

    /// The id is already registered. Re-registration is rejected
    /// unconditionally — even with the same impl — because
    /// `Arc<dyn>` and `&'static dyn` have different fat-pointer
    /// layouts, so we can't cheaply detect "same impl" without
    /// extra bookkeeping. Hosts should `Once`-gate their startup
    /// registration paths.
    #[error("component id {0} is already registered")]
    AlreadyRegistered(ComponentId),
}

/// Process-global runtime component registry.
///
/// Stores `Arc<dyn RuntimeComponent>` keyed by `ComponentId`. The
/// adapter wrapping each entry into `&'static dyn ErasedComponent`
/// is also cached so the dispatch path can hand back a stable
/// `&'static` reference. Updates happen at process startup; reads
/// happen on every commit's validation path.
struct RuntimeRegistry {
    inner: RwLock<HashMap<ComponentId, &'static dyn ErasedComponent>>,
}

impl RuntimeRegistry {
    fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }

    fn register(
        &self,
        component: Arc<dyn RuntimeComponent>,
    ) -> Result<(), RuntimeRegistrationError> {
        let id = component.id();
        if !id.is_app_range() {
            return Err(RuntimeRegistrationError::OutOfRange(id));
        }
        let mut guard = self.inner.write();
        if let Some(_existing) = guard.get(&id) {
            // Bypass the duplicate check if pointer-equal: re-
            // registering the same impl is harmless and lets hosts
            // call `register_global_runtime_component` from
            // idempotent setup paths.
            //
            // We can't compare the underlying `Arc<dyn ...>`
            // pointers directly because `&'static dyn` and
            // `Arc<dyn>` have different fat-pointer layouts. For
            // now treat any second registration as an error;
            // hosts can `Once`-gate their setup.
            return Err(RuntimeRegistrationError::AlreadyRegistered(id));
        }
        // Leak the adapter to get a `&'static` — runtime components
        // live for the process lifetime by design (registered at
        // startup, never unregistered). `Box::leak` of an `Arc`
        // adapter is the simplest way to mint a static reference
        // that the dispatch table can hand back.
        let adapter: Box<dyn ErasedComponent> = Box::new(RuntimeAdapter(component));
        let leaked: &'static dyn ErasedComponent = Box::leak(adapter);
        guard.insert(id, leaked);
        Ok(())
    }

    fn lookup(&self, id: ComponentId) -> Option<&'static dyn ErasedComponent> {
        self.inner.read().get(&id).copied()
    }
}

fn registry() -> &'static RuntimeRegistry {
    static REGISTRY: OnceLock<RuntimeRegistry> = OnceLock::new();
    REGISTRY.get_or_init(RuntimeRegistry::new)
}

/// Register a host-defined runtime component.
///
/// Call once per component at process startup, before any
/// `Client` operates on a group that may carry this component.
/// Returns `Err(OutOfRange)` if the id is outside `0xC000-0xFEFF`,
/// `Err(AlreadyRegistered)` if a different impl is already
/// registered for the same id.
///
/// The component lives for the process lifetime — there is no
/// `unregister`. Hosts that need to swap impls should restart.
pub fn register_global_runtime_component(
    component: Arc<dyn RuntimeComponent>,
) -> Result<(), RuntimeRegistrationError> {
    registry().register(component)
}

/// Look up a runtime-registered component by id.
///
/// Returns `None` if the id is not in the app range (those go
/// through the static `WELL_KNOWN` dispatch table) or if no host
/// has registered a `RuntimeComponent` for the id yet.
pub(crate) fn lookup_runtime_component(id: ComponentId) -> Option<&'static dyn ErasedComponent> {
    if !id.is_app_range() {
        return None;
    }
    registry().lookup(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_data::component_registry::ComponentOp;

    /// A test-only RuntimeComponent that just passes Bytes through.
    /// Each instance carries its own id so different test cases
    /// don't collide on the global registry.
    struct PassthroughBytes(ComponentId);

    impl RuntimeComponent for PassthroughBytes {
        fn id(&self) -> ComponentId {
            self.0
        }

        fn component_type(&self) -> ComponentType {
            ComponentType::Bytes
        }

        fn apply_update_payload(
            &self,
            payload: &[u8],
            _prior: Option<&[u8]>,
        ) -> Result<Vec<u8>, ComponentTypedError> {
            Ok(payload.to_vec())
        }

        fn expand_to_changes(
            &self,
            op: &AppDataUpdateOperation,
            _prior: Option<&[u8]>,
        ) -> Result<Vec<ExpandedComponentChange>, ComponentTypedError> {
            match op {
                AppDataUpdateOperation::Update(p) => Ok(vec![ExpandedComponentChange {
                    op: ComponentOp::Update,
                    value: Some(p.as_slice().to_vec()),
                }]),
                AppDataUpdateOperation::Remove => Ok(vec![ExpandedComponentChange {
                    op: ComponentOp::Delete,
                    value: None,
                }]),
            }
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_out_of_range_ids() {
        let well_known_id = ComponentId::GROUP_NAME; // XMTP range
        let err = register_global_runtime_component(Arc::new(PassthroughBytes(well_known_id)))
            .unwrap_err();
        assert!(matches!(
            err,
            RuntimeRegistrationError::OutOfRange(id) if id == well_known_id
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn registers_and_dispatches_through_lookup() {
        // Pick a unique id per test run to avoid cross-test
        // collisions on the process-global registry. 0xC100 is
        // arbitrary but stays in the app range.
        let id = ComponentId::new(0xC100);

        // First registration succeeds.
        register_global_runtime_component(Arc::new(PassthroughBytes(id))).unwrap();

        // Lookup returns an ErasedComponent that handles the id.
        let entry = lookup_runtime_component(id).expect("registered component should look up");
        assert_eq!(entry.id(), id);
        assert_eq!(entry.component_type(), ComponentType::Bytes);

        // Apply path works.
        let new = entry.apply_update_payload(b"hello", None).unwrap();
        assert_eq!(new, b"hello");
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn duplicate_registration_rejected() {
        // Use a different id than `registers_and_dispatches_through_lookup`
        // so test ordering doesn't matter.
        let id = ComponentId::new(0xC101);
        register_global_runtime_component(Arc::new(PassthroughBytes(id))).unwrap();

        let err = register_global_runtime_component(Arc::new(PassthroughBytes(id))).unwrap_err();
        assert!(matches!(
            err,
            RuntimeRegistrationError::AlreadyRegistered(seen) if seen == id
        ));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn lookup_returns_none_for_unregistered_id() {
        // 0xC1FF is reserved for "no test ever uses this" — so
        // this lookup deterministically returns None even when
        // tests run in arbitrary order.
        let unregistered = ComponentId::new(0xC1FF);
        assert!(lookup_runtime_component(unregistered).is_none());

        // XMTP-range ids always return None from the runtime path
        // (they're handled by the static dispatch table instead).
        assert!(lookup_runtime_component(ComponentId::GROUP_NAME).is_none());
    }
}
