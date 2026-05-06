//! Typed dispatch for AppData components.
//!
//! Defines the [`Component`] trait that pairs a static [`ComponentId`]
//! with a value type, encoding, and per-component validation hooks.
//! Each well-known component (GROUP_NAME, ADMIN_LIST, etc.) has one
//! `Component` impl living under `app_data::components::*`. Together
//! with the static dispatch table in `app_data::registry_table`, the
//! trait is the single source of truth for what a [`ComponentId`]
//! means at the byte level.
//!
//! ## Type erasure
//!
//! [`Component`] has only static methods (no `&self`), so `dyn Component`
//! cannot be formed. The companion [`ErasedComponent`] trait is the
//! object-safe shape used for runtime dispatch. A blanket impl over any
//! `C: Component + Send + Sync + 'static` lets each Component impl be a
//! zero-sized type — `&'static dyn ErasedComponent` is a single
//! pointer with no per-call boxing.
//!
//! ## Bytes-out, value-out boundary
//!
//! The trait deliberately splits "byte-in / byte-out" methods (which
//! survive type erasure) from "value-in / value-out" methods (which
//! require the static `Self::Value` / `Self::Mutation` types).
//! Validators and the steady-state apply path use the byte-shaped
//! methods through `&dyn ErasedComponent`; reads against a known
//! component impl use `decode_value` directly with the static type
//! through the [`MlsGroupAppData`](super) facade in `xmtp_mls`.

use openmls::messages::proposals::AppDataUpdateOperation;
use xmtp_proto::xmtp::mls::message_contents::ComponentType;

use crate::{
    app_data::{
        component_id::ComponentId,
        component_registry::{ComponentOp, ComponentRegistry},
        validation::ComponentChange,
    },
    inbox_id::InboxIdError,
    tls_map::TlsMapError,
    tls_set::TlsSetError,
};

/// A typed view of a well-known AppData component.
///
/// One impl per [`ComponentId`] — pairs the wire identifier with the
/// component's logical type, decode/encode round-trip, mutation
/// serialization, in-place apply behavior for incoming Update payloads,
/// and (optionally) component-local invariant checks.
///
/// The registry's permission check (`validate_component_write` in
/// `app_data::validation`) is cross-cutting and runs outside this
/// trait. [`Component::validate_invariant`] is for component-LOCAL
/// invariants (e.g. "GROUP_NAME ≤ 1 KiB", "ADMIN_LIST always has at
/// least one super-admin mirror") that the policy evaluator can't
/// express.
pub trait Component: Send + Sync + 'static {
    /// Stable wire identifier. Const-known so registries can build
    /// static lookup tables.
    const ID: ComponentId;

    /// Logical wire type, written into the registry's
    /// [`ComponentMetadata.component_type`] slot at bootstrap.
    ///
    /// [`ComponentMetadata.component_type`]: xmtp_proto::xmtp::mls::message_contents::ComponentMetadata
    const COMPONENT_TYPE: ComponentType;

    /// Decoded full-state value (e.g. `String`, `TlsSet<InboxId>`,
    /// `TlsMap<InboxId, VLBytes>`).
    type Value;

    /// Decoded payload that goes inside an
    /// `AppDataUpdateOperation::Update`.
    ///
    /// For Bytes/String components this is the same as `Value` (a
    /// full-value replacement). For collection components this is
    /// the wire-level **delta** (`TlsSetDelta<K>` /
    /// `TlsMapDelta<K, V>`) carrying one *or more* mutations — every
    /// mutation in the delta lands as one atomic change at the
    /// receiver. Single-mutation callers build a one-element delta
    /// via the fluent builder (`TlsSetDelta::new().insert(x)`).
    ///
    /// Batching matters for components like `GROUP_MEMBERSHIP` where
    /// all installation changes for an inbox (additions, removals,
    /// new installations) must travel in a single proposal so the
    /// receiver applies them atomically.
    type Mutation;

    /// Decode the component's stored bytes (as found in the AppData
    /// dictionary) into the typed value.
    fn decode_value(bytes: &[u8]) -> Result<Self::Value, ComponentTypedError>;

    /// Encode a typed value back to the bytes that go in the AppData
    /// dictionary slot. The inverse of [`Component::decode_value`].
    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, ComponentTypedError>;

    /// Encode a [`Self::Mutation`] as the bytes that go inside an
    /// `AppDataUpdateOperation::Update` payload.
    ///
    /// Bytes/String components pass through; collection components
    /// serialize the delta (which may carry multiple mutations — see
    /// the [`Self::Mutation`] doc on batching).
    fn encode_mutation(mutation: &Self::Mutation) -> Result<Vec<u8>, ComponentTypedError>;

    /// Apply an `AppDataUpdateOperation::Update` payload against the
    /// component's prior bytes (or `None` if first write) and return
    /// the new full bytes.
    ///
    /// Bytes/String components ignore `prior` and pass through;
    /// collection components decode `payload` as a delta and apply it
    /// to the decoded prior set/map.
    fn apply_update_payload(
        payload: &[u8],
        prior: Option<&[u8]>,
    ) -> Result<Vec<u8>, ComponentTypedError>;

    /// Expand an `AppDataUpdate` proposal (Update or Remove) into the
    /// per-element changes the validator's policy loop iterates over.
    ///
    /// Bytes components produce exactly one entry; collection
    /// components produce one entry per delta mutation.
    fn expand_to_changes(
        op: &AppDataUpdateOperation,
        prior: Option<&[u8]>,
    ) -> Result<Vec<ExpandedComponentChange>, ComponentTypedError>;

    /// Optional component-local invariant check that runs *after*
    /// `validate_component_write`'s policy verdict. Default is no-op
    /// — components with no extra invariants don't override.
    fn validate_invariant(
        _change: &ComponentChange<'_>,
        _registry: &ComponentRegistry,
    ) -> Result<(), ComponentInvariantError> {
        Ok(())
    }
}

/// Object-safe view of a [`Component`] impl, for runtime dispatch via
/// `&'static dyn ErasedComponent`. Carries only the byte-shaped
/// methods — typed reads through `decode_value` / `encode_value` /
/// `encode_mutation` need the static `Self::Value` / `Self::Mutation`
/// types and aren't reachable through the dyn boundary.
///
/// A blanket impl over `C: Component` makes every concrete Component
/// impl auto-implement this trait.
pub trait ErasedComponent: Send + Sync + 'static {
    /// The component this impl handles — equivalent to `C::ID`.
    fn id(&self) -> ComponentId;

    /// The wire type — equivalent to `C::COMPONENT_TYPE`.
    fn component_type(&self) -> ComponentType;

    fn apply_update_payload(
        &self,
        payload: &[u8],
        prior: Option<&[u8]>,
    ) -> Result<Vec<u8>, ComponentTypedError>;

    fn expand_to_changes(
        &self,
        op: &AppDataUpdateOperation,
        prior: Option<&[u8]>,
    ) -> Result<Vec<ExpandedComponentChange>, ComponentTypedError>;

    fn validate_invariant(
        &self,
        change: &ComponentChange<'_>,
        registry: &ComponentRegistry,
    ) -> Result<(), ComponentInvariantError>;
}

impl<C: Component> ErasedComponent for C {
    fn id(&self) -> ComponentId {
        C::ID
    }

    fn component_type(&self) -> ComponentType {
        C::COMPONENT_TYPE
    }

    fn apply_update_payload(
        &self,
        payload: &[u8],
        prior: Option<&[u8]>,
    ) -> Result<Vec<u8>, ComponentTypedError> {
        C::apply_update_payload(payload, prior)
    }

    fn expand_to_changes(
        &self,
        op: &AppDataUpdateOperation,
        prior: Option<&[u8]>,
    ) -> Result<Vec<ExpandedComponentChange>, ComponentTypedError> {
        C::expand_to_changes(op, prior)
    }

    fn validate_invariant(
        &self,
        change: &ComponentChange<'_>,
        registry: &ComponentRegistry,
    ) -> Result<(), ComponentInvariantError> {
        C::validate_invariant(change, registry)
    }
}

/// A single per-element view of an incoming `AppDataUpdate` proposal.
///
/// `Bytes` components produce exactly one entry; collection components
/// produce one entry per delta mutation. The `op` mirrors the
/// `ComponentOp` field on a [`ComponentChange`] so the validator can
/// call `validate_component_write` directly.
///
/// `value` is `None` for `Delete` ops on collection components when
/// the receiver removes by key (e.g. unresolvable `RemoveByHash`); for
/// every other case it is `Some` with the new value bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedComponentChange {
    /// Whether this entry is an Insert, Update, or Delete.
    pub op: ComponentOp,
    /// The new value bytes for Insert/Update, or `None` for Delete
    /// when the value is not available.
    pub value: Option<Vec<u8>>,
}

/// Errors surfaced by [`Component`] trait methods.
///
/// Narrower than `ComponentSourceError` (which lives one layer up at
/// the dispatch boundary in `xmtp_mls`). The dispatch wrapper converts
/// these into the source-layer error via `From<ComponentTypedError>`.
#[derive(Debug, thiserror::Error)]
pub enum ComponentTypedError {
    /// An `AppDataUpdate::Update` write was attempted against an
    /// immutable component. Insert-once writes should be expressed as
    /// `Insert`, not caught here.
    #[error("component {0} is immutable and cannot be updated via AppDataUpdate")]
    ImmutableUpdate(ComponentId),

    /// The supplied mutation does not match the component type of the
    /// component it targets (e.g. a Bytes mutation against a Set
    /// component).
    #[error("mutation shape does not match component {0}")]
    MismatchedMutation(ComponentId),

    /// A wire-format violation: the bytes passed to
    /// [`Component::decode_value`] or
    /// [`Component::apply_update_payload`] don't decode under the
    /// component's expected encoding.
    #[error("malformed value for component {component_id}: {reason}")]
    MalformedValue {
        component_id: ComponentId,
        reason: String,
    },

    /// Failed to convert an inbox id string or byte slice into an
    /// [`InboxId`](crate::inbox_id::InboxId).
    #[error("invalid inbox id: {0}")]
    InvalidInboxId(#[from] InboxIdError),

    /// A TLS-codec operation on a delta or stored collection value
    /// failed.
    #[error("tls codec error: {0}")]
    TlsCodec(#[from] tls_codec::Error),

    /// A `TlsSet::apply_delta` call failed while synthesizing the new
    /// full value of a Set component from an incoming delta.
    #[error("tls set apply error: {0}")]
    TlsSetApply(#[from] TlsSetError),

    /// A `TlsMap::apply_delta` call failed while synthesizing the new
    /// full value of a Map component from an incoming delta.
    #[error("tls map apply error: {0}")]
    TlsMapApply(#[from] TlsMapError),
}

/// Errors surfaced by [`Component::validate_invariant`].
///
/// Component-local invariants — e.g. "GROUP_NAME ≤ N bytes",
/// "removing the last super-admin mirror is forbidden". Distinct from
/// [`ComponentPermissionError`](crate::app_data::validation::ComponentPermissionError),
/// which is the policy-evaluation verdict.
#[derive(Debug, thiserror::Error)]
pub enum ComponentInvariantError {
    /// A component-specific invariant was violated. The string is a
    /// short human-readable diagnostic; structured error data can be
    /// added later if a caller needs to inspect.
    #[error("component {component_id} invariant violated: {reason}")]
    Violation {
        component_id: ComponentId,
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_data::{component_registry::ComponentOp, validation::ActorAuthority};

    /// A minimal Component impl exercised only by these unit tests —
    /// confirms the trait shape is usable without the real well-known
    /// impls (which arrive in subsequent jj changes).
    struct DummyBytesComponent;

    impl Component for DummyBytesComponent {
        const ID: ComponentId = ComponentId::APP_DATA;
        const COMPONENT_TYPE: ComponentType = ComponentType::Bytes;
        type Value = Vec<u8>;
        type Mutation = Vec<u8>;

        fn decode_value(bytes: &[u8]) -> Result<Self::Value, ComponentTypedError> {
            Ok(bytes.to_vec())
        }

        fn encode_value(value: &Self::Value) -> Result<Vec<u8>, ComponentTypedError> {
            Ok(value.clone())
        }

        fn encode_mutation(mutation: &Self::Mutation) -> Result<Vec<u8>, ComponentTypedError> {
            Ok(mutation.clone())
        }

        fn apply_update_payload(
            payload: &[u8],
            _prior: Option<&[u8]>,
        ) -> Result<Vec<u8>, ComponentTypedError> {
            Ok(payload.to_vec())
        }

        fn expand_to_changes(
            op: &AppDataUpdateOperation,
            _prior: Option<&[u8]>,
        ) -> Result<Vec<ExpandedComponentChange>, ComponentTypedError> {
            match op {
                AppDataUpdateOperation::Update(payload) => Ok(vec![ExpandedComponentChange {
                    op: ComponentOp::Update,
                    value: Some(payload.as_slice().to_vec()),
                }]),
                AppDataUpdateOperation::Remove => Ok(vec![ExpandedComponentChange {
                    op: ComponentOp::Delete,
                    value: None,
                }]),
            }
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn typed_methods_round_trip() {
        let v = b"hello".to_vec();
        let bytes = DummyBytesComponent::encode_value(&v).unwrap();
        let decoded = DummyBytesComponent::decode_value(&bytes).unwrap();
        assert_eq!(decoded, v);
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn erased_component_blanket_impl() {
        // Static dispatch through &dyn ErasedComponent works without
        // per-call boxing because the impl is over a ZST.
        let erased: &'static dyn ErasedComponent = &DummyBytesComponent;
        assert_eq!(erased.id(), ComponentId::APP_DATA);
        assert_eq!(erased.component_type(), ComponentType::Bytes);

        let new = erased.apply_update_payload(b"world", None).unwrap();
        assert_eq!(new, b"world");

        let expanded = erased
            .expand_to_changes(&AppDataUpdateOperation::Update(b"x".to_vec().into()), None)
            .unwrap();
        assert_eq!(expanded.len(), 1);
        assert_eq!(expanded[0].op, ComponentOp::Update);
        assert_eq!(expanded[0].value.as_deref(), Some(b"x".as_slice()));
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn default_validate_invariant_is_noop() {
        let actor = ActorAuthority {
            is_admin: false,
            is_super_admin: true,
        };
        let change = ComponentChange::builder()
            .component_id(ComponentId::APP_DATA)
            .op(ComponentOp::Update)
            .actor(actor)
            .build();
        let registry = ComponentRegistry::new();
        <DummyBytesComponent as Component>::validate_invariant(&change, &registry).unwrap();
    }
}
