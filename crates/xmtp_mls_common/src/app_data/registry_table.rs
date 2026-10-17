//! Static dispatch table for well-known [`Component`] impls.
//!
//! Maps each well-known [`ComponentId`] to its zero-sized
//! [`ErasedComponent`] impl so dispatch sites can resolve a runtime
//! [`ComponentId`] to the right per-component logic without per-call
//! boxing. The table is hand-maintained and sorted by
//! `ComponentId::as_u16()` so [`lookup_component`] does a single
//! binary search.
//!
//! ## Adding a new well-known component
//!
//! 1. Add a `Component` impl in `app_data::components::*`.
//! 2. Insert a `(ComponentId::FOO, &FooComponent)` entry into
//!    [`WELL_KNOWN`], maintaining ascending sort order.
//! 3. The compile-time `assert_table_is_sorted_and_unique` check at
//!    the bottom of this file verifies invariants on every build.
//!
//! Application-range components have no entry and no process-local
//! handler: every client decodes them with the standard codec for the
//! type in the group's committed registry, so no local code can change
//! which commits a client accepts.

use crate::app_data::{
    component_id::ComponentId,
    components::{
        inbox_id_set::{AdminListComponent, DmMembersComponent, SuperAdminListComponent},
        metadata_attributes::{
            AppDataComponent, CommitLogSignerComponent, GroupDescriptionComponent,
            GroupImageUrlComponent, GroupNameComponent, MessageDisappearFromNsComponent,
            MessageDisappearInNsComponent, MinSupportedProtocolVersionComponent,
        },
        tls_map_components::{ComponentRegistryComponent, GroupMembershipComponent},
    },
    typed::ErasedComponent,
};

/// Sorted-ascending table of `(ComponentId, &dyn ErasedComponent)`
/// entries for every well-known XMTP component.
///
/// Order is enforced by [`assert_table_is_sorted_and_unique`] at
/// compile time. Tests further pin specific lookup expectations.
///
/// # Change control
///
/// Registry policy expresses **authority** only: every policy variant is a
/// predicate over the actor. A component whose correctness depends on a
/// predicate over its resulting value, or on a bound on that value, cannot be
/// expressed in the registry and MUST implement
/// [`Component::validate_invariant`](crate::app_data::typed::Component::validate_invariant).
/// Prefer a registry rule whenever it can express the requirement. Add an
/// invariant only when it cannot, and record that justification at the impl.
///
/// Component-id ranges in play (mirror of [`lookup_component`] below):
///
/// | Range            | Purpose                                       |
/// |------------------|-----------------------------------------------|
/// | `0x8000-0xBFFF`  | XMTP-allocated well-known ids (this table)    |
/// | `0xC000-0xFEFF`  | Application ids, decoded by registry type     |
/// | `0xFF00-0xFFFF`  | Reserved (hard-rejected, no graceful-degrade) |
///
/// Adding a new well-known entry here changes the protocol's
/// receiver-side acceptance set. Old clients (released before the new
/// entry) handle the new id via the **type-aware unknown-component
/// tolerance** path in:
///   - `apply_app_data_update_payload`
///   - `expand_app_data_update_to_changes`
///   - `validate_one_app_data_update_with_old_value`
///
/// That path looks the unknown id up in the on-dict
/// [`ComponentRegistry`](crate::app_data::component_registry::ComponentRegistry),
/// pulls its registered [`ComponentType`], and dispatches through the
/// type-level decoder. The closed type universe covers every shape:
/// Bytes / String pass-through, `TlsSet<InboxId>` / `TlsSet<bytes>` /
/// `TlsMap<InboxId, bytes>` / `TlsMap<InboxId, UTF-8 bytes>` /
/// `TlsMap<bytes, bytes>` apply their deltas element-wise — old and new
/// clients converge on the same dict bytes.
/// The tolerance path covers the XMTP range (`0x8000-0xBFFF`) and the
/// application range (`0xC000-0xFEFF`); the reserved range
/// (`0xFF00-0xFFFF`) is **still hard-rejected** — those slots are
/// protocol-level and have no graceful-degrade story. Do not allocate
/// new ids there.
///
/// **Requirements when adding a new well-known component:**
/// - The component MUST be reachable through one of the seven
///   [`ComponentType`] variants. The wire codec for each is fixed; an
///   old client decodes it the same way a typed client would.
/// - The component MUST NOT carry receive-side invariants beyond
///   registry policy. Old (type-dispatched) clients lack the per-id
///   `Component::validate_invariant` hook — diverging invariant
///   behavior would fork the dict.
/// - Read-side accessors surface **the default value for the
///   component's type** on old clients (empty `Bytes` / `String` /
///   `TlsSet` / `TlsMap`). The "absent" state is indistinguishable
///   from "explicitly cleared" on old clients — design semantics
///   accordingly and document that degradation at the accessor
///   boundary.
/// - The component MUST land in the registry **before or with** the
///   first commit that writes to it. Old clients consult the
///   pre-commit registry snapshot, so a same-commit registration
///   followed by a same-commit write fails to dispatch.
///
/// **Floor-bump convention (pause, don't fork).** Any release that
/// introduces something old receivers cannot interpret — a new
/// [`ComponentType`], a new set/map delta mutation tag, a new registry
/// entry format, a reserved-range (`0xFF00+`) allocation, or a change
/// to the bootstrap synthesis encoding — MUST raise
/// `PROPOSALS_MIN_PROTOCOL_VERSION` in the same release AND land each
/// group's `MIN_SUPPORTED_PROTOCOL_VERSION` floor bump in a commit
/// **strictly earlier** than the first commit using the new construct
/// (the floor-bump commit itself must contain nothing format-novel).
/// Receivers below a committed floor pause the group
/// (defer-and-reprocess after upgrade) via the pause-before-parse
/// guards in `xmtp_mls::groups::app_data` and
/// `ValidatedCommit::from_staged_commit`; a same-commit floor bump is
/// NOT protected — its proposal hasn't passed the super-admin policy
/// check when the guards run, and pausing on unvalidated input would
/// let any member freeze a group.
///
/// Two ergonomic patterns for shipping a new component without
/// editing `WELL_KNOWN`:
///
/// 1. **Application-range component.** A component in
///    `0xC000-0xFEFF` registered in a group's `COMPONENT_REGISTRY`
///    with one of the standard types ships without touching
///    `WELL_KNOWN`; every client type-dispatches it via the registry.
/// 2. **Coordinated protocol-version bump.** Required only when the
///    new component must reject specific bytes that the type-level
///    codec would otherwise accept (e.g. a per-id invariant beyond
///    type shape).
///
/// [`ComponentType`]: xmtp_proto::xmtp::mls::message_contents::ComponentType
pub static WELL_KNOWN: &[(ComponentId, &'static dyn ErasedComponent)] = &[
    (ComponentId::COMPONENT_REGISTRY, &ComponentRegistryComponent),
    (ComponentId::SUPER_ADMIN_LIST, &SuperAdminListComponent),
    (ComponentId::ADMIN_LIST, &AdminListComponent),
    (ComponentId::GROUP_MEMBERSHIP, &GroupMembershipComponent),
    (ComponentId::GROUP_NAME, &GroupNameComponent),
    (ComponentId::GROUP_DESCRIPTION, &GroupDescriptionComponent),
    (ComponentId::GROUP_IMAGE_URL, &GroupImageUrlComponent),
    (
        ComponentId::MESSAGE_DISAPPEAR_FROM_NS,
        &MessageDisappearFromNsComponent,
    ),
    (
        ComponentId::MESSAGE_DISAPPEAR_IN_NS,
        &MessageDisappearInNsComponent,
    ),
    (ComponentId::APP_DATA, &AppDataComponent),
    (
        ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION,
        &MinSupportedProtocolVersionComponent,
    ),
    (ComponentId::COMMIT_LOG_SIGNER, &CommitLogSignerComponent),
    (ComponentId::DM_MEMBERS, &DmMembersComponent),
];

/// Look up the well-known [`ErasedComponent`] for a [`ComponentId`].
///
/// Returns `None` for every application-range and reserved id, and for
/// a well-known id with no impl (e.g. the `0xBE0x` immutable seeds —
/// handled by the bootstrap validator's byte-compare path rather than
/// the trait). Callers then decode by the id's registered type.
// implements: META-010
pub fn lookup_component(id: ComponentId) -> Option<&'static dyn ErasedComponent> {
    WELL_KNOWN
        .binary_search_by_key(&id.as_u16(), |(component_id, _)| component_id.as_u16())
        .ok()
        .map(|idx| WELL_KNOWN[idx].1)
}

/// Compile-time check that [`WELL_KNOWN`] is strictly ascending by
/// `ComponentId::as_u16()` (so [`lookup_component`]'s binary search is
/// correct) and that no entry's declared id disagrees with its impl's
/// `Component::ID`.
///
/// Triggered as `const _: () = assert_table_is_sorted_and_unique();`
/// at module scope below.
const fn assert_table_is_sorted_and_unique() {
    let mut i = 1;
    while i < WELL_KNOWN.len() {
        let prev = WELL_KNOWN[i - 1].0.as_u16();
        let curr = WELL_KNOWN[i].0.as_u16();
        assert!(prev < curr, "WELL_KNOWN must be strictly ascending");
        i += 1;
    }
}

const _: () = assert_table_is_sorted_and_unique();

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_data::typed::Component;
    use xmtp_proto::xmtp::mls::message_contents::ComponentType;

    #[xmtp_common::test(unwrap_try = true)]
    fn lookup_returns_correct_component_for_each_well_known_id() {
        let cases = [
            (
                ComponentId::COMPONENT_REGISTRY,
                ComponentType::TlsMapBytesBytes,
            ),
            (ComponentId::SUPER_ADMIN_LIST, ComponentType::TlsSetInboxId),
            (ComponentId::ADMIN_LIST, ComponentType::TlsSetInboxId),
            (
                ComponentId::GROUP_MEMBERSHIP,
                ComponentType::TlsMapInboxIdBytes,
            ),
            (ComponentId::GROUP_NAME, ComponentType::String),
            (ComponentId::GROUP_DESCRIPTION, ComponentType::String),
            (ComponentId::GROUP_IMAGE_URL, ComponentType::String),
            (ComponentId::MESSAGE_DISAPPEAR_FROM_NS, ComponentType::Bytes),
            (ComponentId::MESSAGE_DISAPPEAR_IN_NS, ComponentType::Bytes),
            (ComponentId::APP_DATA, ComponentType::String),
            (
                ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION,
                ComponentType::String,
            ),
            (ComponentId::COMMIT_LOG_SIGNER, ComponentType::Bytes),
            (ComponentId::DM_MEMBERS, ComponentType::TlsSetInboxId),
        ];
        for (id, expected_type) in cases {
            let entry =
                lookup_component(id).unwrap_or_else(|| panic!("missing dispatch for {id:?}"));
            assert_eq!(entry.id(), id);
            assert_eq!(entry.component_type(), expected_type);
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn lookup_returns_none_for_unknown_id() {
        // Application ids never dispatch to local code.
        for id in [0xC000, 0xC123, 0xFEFF] {
            assert!(lookup_component(ComponentId::new(id)).is_none());
        }

        // Immutable seed without a Component impl yet — bootstrap
        // validator handles it via byte-compare, not the trait.
        assert!(lookup_component(ComponentId::CONVERSATION_TYPE).is_none());
        assert!(lookup_component(ComponentId::CREATOR_INBOX_ID).is_none());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn well_known_entries_match_component_const_id() {
        // Detect copy-paste errors: each table entry's declared id
        // must equal its impl's `Component::ID` (which the
        // ErasedComponent vtable surfaces via `id()`).
        for (declared_id, erased) in WELL_KNOWN {
            assert_eq!(
                erased.id(),
                *declared_id,
                "WELL_KNOWN entry for {declared_id:?} points to an impl with id {:?}",
                erased.id()
            );
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn dispatch_through_erased_calls_typed_apply() {
        // End-to-end: lookup_component returns an &dyn ErasedComponent
        // whose apply_update_payload mirrors the typed Component::apply_update_payload.
        let payload = b"new-name";
        let typed_result =
            <GroupNameComponent as Component>::apply_update_payload(payload, None).unwrap();
        let erased = lookup_component(ComponentId::GROUP_NAME).unwrap();
        let erased_result = erased.apply_update_payload(payload, None).unwrap();
        assert_eq!(typed_result, erased_result);
    }
}
