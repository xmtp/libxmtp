use super::*;
use crate::{
    app_data::{
        component_permissions::component_permissions,
        component_registry::{ComponentOp, new_component_metadata},
    },
    inbox_id::INBOX_ID_BYTE_LEN,
};
use prost::Message;
use xmtp_mls_tls_collections::tls_set::TlsKeyHash;
use xmtp_proto::xmtp::mls::message_contents::{
    MetadataPolicy as MetadataPolicyProto,
    metadata_policy::{Kind as MetadataPolicyKind, MetadataBasePolicy},
};

mod apply_payload;
mod component_types;
mod expand_changes;
mod field_bounds;
mod metadata_reads;

/// Build a deterministic 64-character hex inbox id from a tag byte. The
/// tag is repeated 32 times, giving a unique inbox id per call without
/// needing real cryptographic generation.
fn fake_inbox_id(tag: u8) -> String {
    hex::encode([tag; INBOX_ID_BYTE_LEN])
}

/// Build the [`InboxId`] form of [`fake_inbox_id`] directly.
fn fake_inbox(tag: u8) -> InboxId {
    InboxId::from_bytes([tag; INBOX_ID_BYTE_LEN])
}

/// Empty registry constant for tests that exercise known-id paths —
/// `lookup_component` resolves first, so the registry is never
/// consulted and an empty one is sufficient.
fn empty_registry() -> ComponentRegistry {
    ComponentRegistry::new()
}

/// A one-mutation `TlsSetDelta<InboxId>` payload.
fn inbox_set_delta(mutation: TlsSetMutation<InboxId>) -> Vec<u8> {
    TlsSetDelta {
        mutations: vec![mutation],
    }
    .tls_serialize_detached()
    .unwrap()
}

/// Build a single-entry registry for tests that exercise the
/// type-aware fallback on unknown ids. Permissions are `Allow` for
/// every op so the policy layer does not interfere with the
/// dispatch test under question.
fn registry_with(id: ComponentId, ty: ComponentType) -> ComponentRegistry {
    fn allow() -> MetadataPolicyProto {
        MetadataPolicyProto {
            kind: Some(MetadataPolicyKind::Base(MetadataBasePolicy::Allow as i32)),
        }
    }
    let perms = component_permissions()
        .insert(allow())
        .update(allow())
        .delete(allow())
        .call();
    let meta = new_component_metadata(perms, ty);
    let mut reg = ComponentRegistry::new();
    reg.set(id, meta).unwrap();
    reg
}
