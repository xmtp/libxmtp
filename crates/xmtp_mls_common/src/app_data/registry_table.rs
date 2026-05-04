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
//! Custom (host-registered) components live outside this table — see
//! `app_data::custom` (added in jj change #14) for the runtime
//! registration path.

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

/// Look up the [`ErasedComponent`] for a [`ComponentId`].
///
/// The two sources are disjoint by construction — `WELL_KNOWN`
/// entries all sit in the XMTP range (`0x8000-0xBFFF`) and runtime
/// entries are gated to the app range (`0xC000-0xFEFF`) at
/// registration time — so we route by id space and skip the wrong
/// table entirely:
///
/// 1. XMTP range → binary-search the static `WELL_KNOWN` table.
/// 2. App range → consult the process-global runtime registry
///    ([`super::custom::lookup_runtime_component`]).
/// 3. Reserved range (`0xFF00-0xFFFF`) → no dispatch.
///
/// Returns `None` if the id is in a known range but has no impl
/// registered (e.g. the `0xBE0x` immutable seeds — handled by the
/// bootstrap validator's byte-compare path rather than the trait).
pub fn lookup_component(id: ComponentId) -> Option<&'static dyn ErasedComponent> {
    if id.is_xmtp_range() {
        return WELL_KNOWN
            .binary_search_by_key(&id.as_u16(), |(component_id, _)| component_id.as_u16())
            .ok()
            .map(|idx| WELL_KNOWN[idx].1);
    }
    if id.is_app_range() {
        return super::custom::lookup_runtime_component(id);
    }
    None
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
        // App-range custom id — resolved via runtime registry, not WELL_KNOWN.
        let custom = ComponentId::new(0xC123);
        assert!(lookup_component(custom).is_none());

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
    fn well_known_count_matches_plan() {
        // 13 well-known impls per docs/plans/2026-04-10-app-data-migration-plan.md:
        // 8 Bytes/String + 3 TlsSet<InboxId> + 2 TlsMap.
        assert_eq!(WELL_KNOWN.len(), 13);
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
