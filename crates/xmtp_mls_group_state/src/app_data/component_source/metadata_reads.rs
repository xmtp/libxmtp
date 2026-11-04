//! Reads of group metadata, super admins and membership out of the app-data dictionary
//! and group extensions.

use super::*;

// ============================================================================
// Inbox-id encoding helpers
// ============================================================================
//
// Inbox ids are SHA-256 hashes (see `xmtp_id::associations::member::inbox_id`).
// Their canonical string form is a 64-character hex string. Anything we put
// on the wire through the new `AppDataUpdate` path uses the
// versioned `InboxId` newtype instead — see the module-level docs for
// the rationale and `crate::inbox_id` for the full contract.

/// Decode a hex-string inbox id into an [`InboxId`].
///
/// Returns [`ComponentSourceError::InvalidInboxId`] wrapping either
/// [`InboxIdError::InvalidHex`] (input wasn't hex) or
/// [`InboxIdError::InvalidLength`] (wrong byte length after decoding).
/// Callers that need to distinguish the failure modes can match the
/// inner variant.
pub fn inbox_id_str_to_bytes(inbox_id: &str) -> Result<InboxId, ComponentSourceError> {
    InboxId::from_hex(inbox_id).map_err(Into::into)
}

/// Read the super-admin list from the AppData dictionary.
pub fn read_super_admin_list_from_dict(
    mls_group: &OpenMlsGroup,
) -> Result<Option<Vec<String>>, ComponentSourceError> {
    read_super_admin_list_from_extensions(mls_group.extensions())
}

/// Extensions-only variant of [`read_super_admin_list_from_dict`]. Use
/// the shim above when an `OpenMlsGroup` is at hand; this form is
/// available primarily for unit testing and for commit-validation
/// paths that only carry an `Extensions` reference.
pub fn read_super_admin_list_from_extensions(
    extensions: &Extensions<GroupContext>,
) -> Result<Option<Vec<String>>, ComponentSourceError> {
    let Some(ext) = extensions.app_data_dictionary() else {
        return Ok(None);
    };
    let Some(bytes) = ext
        .dictionary()
        .get(&ComponentId::SUPER_ADMIN_LIST.as_u16())
    else {
        return Ok(None);
    };
    let set = TlsSet::<InboxId>::tls_deserialize_exact(bytes).map_err(|e| {
        ComponentSourceError::MalformedComponentValue {
            component_id: ComponentId::SUPER_ADMIN_LIST,
            reason: format!("invalid TlsSet<InboxId>: {e}"),
        }
    })?;
    Ok(Some(set.iter().map(|id| id.to_hex()).collect()))
}

/// Synthesize a [`GroupMetadata`] from the AppData dictionary.
/// Returns `Ok(None)` if the critical immutable seeds are absent.
///
/// Encoding mirrors the writer, [`crate::app_data::creation::initial_dictionary`]:
/// - `CONVERSATION_TYPE`: 4 big-endian bytes of `ConversationType as i32`
///   (see `encode_conversation_type` there).
/// - `CREATOR_INBOX_ID`: the versioned `InboxId` TLS wire form
///   (`varint(version) || 32-byte payload`) — the same shape every
///   other inbox-id-bearing component on the new path uses. Reader
///   hex-encodes the decoded id back into the legacy
///   `GroupMetadata::creator_inbox_id: String` slot.
/// - `DM_MEMBERS`: `TlsSet<InboxId>` with exactly two elements —
///   matches the declared `ComponentType::TlsSetInboxId`. The writer
///   rejects self-DMs (identical slots) up front; readers that see a
///   1-element set surface `MalformedComponentValue`.
/// - `ONESHOT_MESSAGE`: prost-encoded `OneshotMessage`.
pub fn read_group_metadata_from_dict(
    mls_group: &OpenMlsGroup,
) -> Result<Option<GroupMetadataReturn>, ComponentSourceError> {
    read_group_metadata_from_extensions(mls_group.extensions())
}

/// Extensions-only variant of [`read_group_metadata_from_dict`]. Same
/// rationale for the split as [`read_super_admin_list_from_extensions`].
pub fn read_group_metadata_from_extensions(
    extensions: &Extensions<GroupContext>,
) -> Result<Option<GroupMetadataReturn>, ComponentSourceError> {
    use prost::Message;
    use xmtp_proto::xmtp::mls::message_contents::{
        ConversationType, DmMembers as DmMembersProto, Inbox as InboxProto, OneshotMessage,
    };

    let Some(ext) = extensions.app_data_dictionary() else {
        return Ok(None);
    };
    let dict = ext.dictionary();

    let Some(ct_bytes) = dict.get(&ComponentId::CONVERSATION_TYPE.as_u16()) else {
        return Ok(None);
    };
    let Some(creator_bytes) = dict.get(&ComponentId::CREATOR_INBOX_ID.as_u16()) else {
        return Ok(None);
    };

    let ct_arr: [u8; 4] =
        ct_bytes
            .try_into()
            .map_err(|_| ComponentSourceError::MalformedComponentValue {
                component_id: ComponentId::CONVERSATION_TYPE,
                reason: format!("expected 4 bytes, got {}", ct_bytes.len()),
            })?;
    let conversation_type = i32::from_be_bytes(ct_arr);

    let creator_inbox_id = InboxId::tls_deserialize_exact(creator_bytes)
        .map_err(|e| ComponentSourceError::MalformedComponentValue {
            component_id: ComponentId::CREATOR_INBOX_ID,
            reason: format!("invalid InboxId TLS encoding: {e}"),
        })?
        .to_hex();

    // `DM_MEMBERS` on the wire is `TlsSet<InboxId>`; re-shape to
    // `DmMembersProto` so downstream `GroupMetadata::try_from` is unchanged.
    // The pair grants DM-only authority, so it is malformed outside a DM.
    // implements: META-030
    let dm_members = match dict.get(&ComponentId::DM_MEMBERS.as_u16()) {
        Some(_) if conversation_type != ConversationType::Dm as i32 => {
            return Err(ComponentSourceError::MalformedComponentValue {
                component_id: ComponentId::DM_MEMBERS,
                reason: format!("present in conversation type {conversation_type}"),
            });
        }
        Some(b) => {
            let set = TlsSet::<InboxId>::tls_deserialize_exact(b).map_err(|e| {
                ComponentSourceError::MalformedComponentValue {
                    component_id: ComponentId::DM_MEMBERS,
                    reason: format!("invalid TlsSet<InboxId>: {e}"),
                }
            })?;
            let ids: Vec<InboxId> = set.iter().copied().collect();
            if ids.len() != 2 {
                return Err(ComponentSourceError::MalformedComponentValue {
                    component_id: ComponentId::DM_MEMBERS,
                    reason: format!("expected 2 inbox ids, got {}", ids.len()),
                });
            }
            Some(DmMembersProto {
                dm_member_one: Some(InboxProto {
                    inbox_id: ids[0].to_hex(),
                }),
                dm_member_two: Some(InboxProto {
                    inbox_id: ids[1].to_hex(),
                }),
            })
        }
        None => None,
    };

    let oneshot = match dict.get(&ComponentId::ONESHOT_MESSAGE.as_u16()) {
        Some(b) => Some(OneshotMessage::decode(b).map_err(|e| {
            ComponentSourceError::MalformedComponentValue {
                component_id: ComponentId::ONESHOT_MESSAGE,
                reason: format!("OneshotMessage prost decode: {e}"),
            }
        })?),
        None => None,
    };

    Ok(Some(GroupMetadataReturn {
        conversation_type,
        creator_inbox_id,
        dm_members,
        oneshot,
    }))
}

/// Intermediate proto-shaped result of [`read_group_metadata_from_extensions`].
/// Caller converts to the final [`crate::group_metadata::GroupMetadata`].
#[derive(Debug)]
pub struct GroupMetadataReturn {
    pub conversation_type: i32,
    pub creator_inbox_id: String,
    pub dm_members: Option<xmtp_proto::xmtp::mls::message_contents::DmMembers>,
    pub oneshot: Option<xmtp_proto::xmtp::mls::message_contents::OneshotMessage>,
}

/// Read the `GROUP_MEMBERSHIP` dict entry and decode it into the
/// legacy `GroupMembership` proto shape. Used by `extract_group_membership` on the
/// receive-side validator to bridge the dict-stored membership back
/// into the existing `GroupMembership` Rust type without rewriting
/// every caller.
pub fn read_group_membership_from_dict(
    extensions: &Extensions<GroupContext>,
) -> Result<Option<xmtp_proto::xmtp::mls::message_contents::GroupMembership>, ComponentSourceError>
{
    use crate::app_data::migration::decode_group_membership_dict;
    use xmtp_proto::xmtp::mls::message_contents::GroupMembership as GroupMembershipProto;

    let Some(ext) = extensions.app_data_dictionary() else {
        return Ok(None);
    };
    let Some(bytes) = ext
        .dictionary()
        .get(&ComponentId::GROUP_MEMBERSHIP.as_u16())
    else {
        return Ok(None);
    };

    let entries = decode_group_membership_dict(bytes).map_err(|e| {
        ComponentSourceError::MalformedComponentValue {
            component_id: ComponentId::GROUP_MEMBERSHIP,
            reason: format!("TlsMap decode: {e}"),
        }
    })?;

    // Flatten per-inbox GroupMembershipEntryV1 back into the legacy
    // proto shape: members (inbox_id → sequence_id), failed_installations
    // (flat Vec). The proto still has a flat failed_installations field
    // for backward compat — we concatenate per-inbox failed lists for
    // callers that still read the flat list.
    //
    // `decode_group_membership_dict` already rejects entries with
    // `version: None` (`MigrationError::GroupMembershipEntryUnknownVersion`),
    // so the only legal post-decode shape today is `Some(Version::V1(_))`.
    // Anything else (a future Version variant we can't interpret) is a
    // forward-compat hazard and surfaces as `MalformedComponentValue`.
    use xmtp_proto::xmtp::mls::message_contents::group_membership_entry::Version as GroupMembershipEntryVersion;
    let mut members: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    let mut failed: Vec<Vec<u8>> = Vec::new();
    for (inbox_id, entry) in entries {
        let v1 = match entry.version {
            Some(GroupMembershipEntryVersion::V1(v1)) => v1,
            None => {
                return Err(ComponentSourceError::MalformedComponentValue {
                    component_id: ComponentId::GROUP_MEMBERSHIP,
                    reason: format!(
                        "GroupMembershipEntry for {} has no version",
                        inbox_id.to_hex()
                    ),
                });
            }
        };
        members.insert(inbox_id.to_hex(), v1.sequence_id);
        failed.extend(v1.failed_installations);
    }

    Ok(Some(GroupMembershipProto {
        members,
        failed_installations: failed,
    }))
}

/// Encode a list of hex inbox ids as a TLS-serialized `TlsSet<InboxId>`.
// Only tests call it. `expect` (not `allow`) so the compiler flags this
// once a non-test caller in this module uses it.
#[cfg_attr(not(test), expect(dead_code))]
pub(super) fn encode_inbox_id_set(inbox_ids: &[String]) -> Result<Vec<u8>, ComponentSourceError> {
    let ids: Vec<InboxId> = inbox_ids
        .iter()
        .map(|s| inbox_id_str_to_bytes(s))
        .collect::<Result<Vec<_>, _>>()?;
    let set: TlsSet<InboxId> = ids.into_iter().collect();
    Ok(set.tls_serialize_detached()?)
}
