//! Shared types, errors, and encoding helpers for the app-data
//! migration synthesis path.

use std::collections::BTreeMap;

use prost::Message as _;
use tls_codec::{Deserialize, Serialize, VLBytes};

use xmtp_proto::xmtp::mls::message_contents::GroupMembershipEntry;

use crate::{
    inbox_id::{InboxId, InboxIdError},
    tls_map::{TlsMapDelta, TlsMapMutation},
};

/// Errors produced by the synthesis functions in this module.
#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    /// A required `PolicySet` field was `None`. Both production presets
    /// populate every field, so this only fires on corrupt input.
    #[error("legacy PolicySet is missing required policy field: {0}")]
    MissingPolicyField(&'static str),

    /// `update_metadata_policy` referenced a metadata field we don't
    /// recognize. Silently dropping it would lose permission
    /// enforcement, so synthesis fails fast.
    #[error("update_metadata_policy references unknown metadata field: {0}")]
    UnknownMetadataField(String),

    /// `add_admin_policy`/`remove_admin_policy` wasn't admin- or
    /// super-admin-gated; the constrained-component `MetadataPolicy`
    /// shape can't represent it.
    #[error("ADMIN_LIST admin policy is not admin-or-super-admin (got base={0:?})")]
    NonConstrainedAdminPolicy(Option<i32>),

    /// `update_permissions_policy` must be `AllowIfSuperAdmin` —
    /// `COMPONENT_REGISTRY` is hardcoded super-admin-only on the
    /// receiver, so any other value would silently disagree.
    #[error("update_permissions_policy must be AllowIfSuperAdmin (got {0:?})")]
    UpdatePermissionsNotSuperAdmin(Option<i32>),

    #[error("component registry error: {0}")]
    Registry(#[from] crate::app_data::component_registry::ComponentRegistryError),

    #[error("legacy mutable-metadata extension missing from group")]
    MissingMutableMetadataExtension,

    #[error("legacy group-membership extension missing from group")]
    MissingGroupMembershipExtension,

    #[error("legacy GroupMembership extension decode error: {0}")]
    GroupMembershipDecode(#[from] prost::DecodeError),

    /// Kept distinct from `GroupMembershipDecode` so incident-response
    /// greps land on the right extension type.
    #[error("legacy GroupMutablePermissionsV1 decode error: {0}")]
    GroupPermissionsDecode(prost::DecodeError),

    #[error("legacy GroupMutableMetadata decode error: {0}")]
    MutableMetadataDecode(#[from] crate::group_mutable_metadata::GroupMutableMetadataError),

    #[error("legacy GroupMetadata decode error: {0}")]
    GroupMetadataDecode(#[from] crate::group_metadata::GroupMetadataError),

    #[error("TLS codec error: {0}")]
    TlsCodec(#[from] tls_codec::Error),

    #[error("invalid inbox id: {0}")]
    InvalidInboxId(#[from] InboxIdError),

    /// A `GROUP_MEMBERSHIP` membership-policy variant we can't translate
    /// onto `MetadataPolicyProto`. Mirrors `UnknownMetadataField` —
    /// silently collapsing to Deny would lose enforcement.
    #[error("unrecognized GROUP_MEMBERSHIP policy (base={0:?})")]
    UnknownMembershipPolicy(Option<i32>),

    #[error("invalid CONVERSATION_TYPE payload length: expected 4, got {0}")]
    ConversationTypePayloadLength(usize),

    /// Legacy `GroupMetadata.dm_members` had the same inbox in both slots.
    /// `TlsSet<InboxId>` (the `DM_MEMBERS` wire encoding) dedupes by
    /// value and would silently collapse this to one element, so we
    /// fail loud instead of destroying information.
    #[error("DmMembers self-reference: both slots contain inbox id {0}")]
    DmMembersSelfReference(String),

    /// `GroupMembershipEntry` envelope decoded but the `version` oneof
    /// was unset (or set to a variant this build doesn't recognize).
    /// Treated as a hard decode failure rather than a silent skip.
    #[error("GroupMembershipEntry envelope has unknown or unset version")]
    GroupMembershipEntryUnknownVersion,

    /// A bootstrap-time `GROUP_MEMBERSHIP` delta carried a non-`Insert`
    /// mutation. Bootstrap is "delta from empty," so anything but
    /// `Insert` means the sender or fixture is malformed.
    #[error("GROUP_MEMBERSHIP bootstrap delta carried a non-Insert mutation")]
    GroupMembershipNonInsertBootstrapMutation,
}

/// Encode the bootstrap-time `GROUP_MEMBERSHIP` payload as a
/// `TlsMapDelta<InboxId, VLBytes>` of all-`Insert` mutations — one per
/// inbox, each value a [`GroupMembershipEntry`] envelope (currently
/// always wrapping a `V1`).
///
/// Bootstrap is the first delta against an empty `TlsMap`, so all
/// mutations are inserts. Post-bootstrap updates use the same
/// `TlsMapDelta` wire format with mixed `Insert`/`Update`/`Delete`
/// mutations — same encode/decode path, no snapshot vs. delta split.
pub fn encode_group_membership_delta(
    entries: &BTreeMap<InboxId, GroupMembershipEntry>,
) -> Result<Vec<u8>, MigrationError> {
    let mut delta: TlsMapDelta<InboxId, VLBytes> = TlsMapDelta::new();
    for (inbox_id, entry) in entries {
        delta = delta.insert(*inbox_id, VLBytes::new(entry.encode_to_vec()));
    }
    Ok(delta.tls_serialize_detached()?)
}

/// Decode the bootstrap-time `GROUP_MEMBERSHIP` payload back to a
/// `BTreeMap<InboxId, GroupMembershipEntryV1>` by walking the
/// `TlsMapDelta` mutations. All mutations must be `Insert` (bootstrap
/// is delta-from-empty); anything else surfaces
/// [`MigrationError::GroupMembershipNonInsertBootstrapMutation`].
pub fn decode_group_membership_delta(
    bytes: &[u8],
) -> Result<BTreeMap<InboxId, GroupMembershipEntry>, MigrationError> {
    let delta = TlsMapDelta::<InboxId, VLBytes>::tls_deserialize_exact(bytes)?;
    let mut out: BTreeMap<InboxId, GroupMembershipEntry> = BTreeMap::new();
    for mutation in delta.mutations {
        match mutation {
            TlsMapMutation::Insert { key, value } => {
                let envelope = GroupMembershipEntry::decode(value.as_slice())?;
                if envelope.version.is_none() {
                    return Err(MigrationError::GroupMembershipEntryUnknownVersion);
                }
                out.insert(key, envelope);
            }
            TlsMapMutation::Update { .. } | TlsMapMutation::Delete { .. } => {
                return Err(MigrationError::GroupMembershipNonInsertBootstrapMutation);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use xmtp_proto::xmtp::mls::message_contents::group_membership_entry::{
        V1 as GroupMembershipEntryV1, Version as GroupMembershipEntryVersion,
    };

    #[test]
    fn group_membership_encode_round_trip() {
        let mut entries: BTreeMap<InboxId, GroupMembershipEntryV1> = BTreeMap::new();
        entries.insert(
            InboxId::from_bytes([0x01; 32]),
            GroupMembershipEntryV1 {
                sequence_id: 42,
                failed_installations: vec![vec![0xAA; 16]],
            },
        );
        entries.insert(
            InboxId::from_bytes([0x02; 32]),
            GroupMembershipEntryV1 {
                sequence_id: 99,
                failed_installations: vec![],
            },
        );
        let entries = entries
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    GroupMembershipEntry {
                        version: Some(GroupMembershipEntryVersion::V1(v)),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let bytes = encode_group_membership_delta(&entries).unwrap();
        let decoded = decode_group_membership_delta(&bytes).unwrap();
        assert_eq!(decoded, entries);
    }
}
