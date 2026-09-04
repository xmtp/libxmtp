use openmls::{
    extensions::{Extension, Extensions, UnknownExtension},
    group::{GroupContext, MlsGroup as OpenMlsGroup},
};
use prost::Message;
use std::{collections::HashMap, fmt};
use thiserror::Error;
use xmtp_cryptography::Secret;
use xmtp_proto::xmtp::mls::message_contents::{
    GroupMutableMetadataV1 as GroupMutableMetadataProto, Inboxes as InboxesProto,
};

use super::group::{DMMetadataOptions, GroupMetadataOptions};
use xmtp_configuration::{
    DEFAULT_GROUP_DESCRIPTION, DEFAULT_GROUP_IMAGE_URL_SQUARE, DEFAULT_GROUP_NAME,
    MUTABLE_METADATA_EXTENSION_ID,
};

/// Errors that can occur when working with GroupMutableMetadata.
#[derive(Debug, Error)]
pub enum GroupMutableMetadataError {
    #[error("serialization: {0}")]
    Serialization(#[from] prost::EncodeError),
    #[error("deserialization: {0}")]
    Deserialization(#[from] prost::DecodeError),
    #[error("missing extension")]
    MissingExtension,
    #[error("mutable extension updates only")]
    NonMutableExtensionUpdate,
    #[error("only one change per update permitted")]
    TooManyUpdates,
    #[error("no changes in this update")]
    NoUpdates,
    #[error("missing metadata field")]
    MissingMetadataField,
    /// A well-known component in the AppData dictionary failed to
    /// decode — surfaced by the migrated-group read paths when a
    /// component's wire bytes can't be parsed.
    ///
    /// Structured rather than a flat `String` so downstream consumers
    /// (bindings, error mapping) can match on the offending
    /// `component_id` to discriminate failure modes without parsing a
    /// display string. `component_id` is `Option` because the upstream
    /// `ComponentSourceError` has a few variants that don't carry one
    /// (e.g. wrapped legacy-metadata errors); those map to `None`. The
    /// inner `reason` is diagnostic-only — typically a formatted
    /// `ComponentSourceError` — and should not be matched against.
    #[error("malformed app-data component {component_id:?}: {reason}")]
    MalformedComponent {
        /// Component whose wire bytes failed to decode. `None` for
        /// errors that don't originate at a specific component.
        component_id: Option<super::app_data::component_id::ComponentId>,
        /// Diagnostic string (display-only; not a stable API).
        reason: String,
    },
}

/// Represents the "updateable" metadata fields for a group.
/// Members ability to update metadata is gated by the group permissions.
///
/// New fields should be added to the `supported_fields` function for Metadata Update Support.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetadataField {
    GroupName,
    Description,
    GroupImageUrlSquare,
    MessageDisappearFromNS,
    MessageDisappearInNS,
    MinimumSupportedProtocolVersion,
    CommitLogSigner,
    AppData,
}

impl MetadataField {
    /// String representations used as keys in the GroupMutableMetadata attributes map.
    pub const fn as_str(&self) -> &'static str {
        match self {
            MetadataField::GroupName => "group_name",
            MetadataField::Description => "description",
            MetadataField::GroupImageUrlSquare => "group_image_url_square",
            MetadataField::MessageDisappearFromNS => "message_disappear_from_ns",
            MetadataField::MessageDisappearInNS => "message_disappear_in_ns",
            MetadataField::MinimumSupportedProtocolVersion => "minimum_supported_protocol_version",
            // Uses SUPER_ADMIN_METADATA_PREFIX ("_") to make this field super-admin only
            MetadataField::CommitLogSigner => "_commit_log_signer",
            MetadataField::AppData => "app_data",
        }
    }
}

impl fmt::Display for MetadataField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Settings for disappearing messages in a conversation.
///
/// # Fields
///
/// * `from_ns` - The timestamp (in nanoseconds) from when messages should be tracked for deletion.
/// * `in_ns` - The duration (in nanoseconds) after which tracked messages will be deleted.
#[derive(Default, Debug, Copy, Clone, PartialEq)]
pub struct MessageDisappearingSettings {
    pub from_ns: i64,
    pub in_ns: i64,
}

impl MessageDisappearingSettings {
    pub fn new(from_ns: i64, in_ns: i64) -> Self {
        Self { from_ns, in_ns }
    }

    pub fn is_enabled(&self) -> bool {
        self.from_ns > 0 && self.in_ns > 0
    }
}

/// Represents the mutable metadata for a group.
///
/// This struct is stored as an MLS Unknown Group Context Extension.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupMutableMetadata {
    /// Map to store various metadata attributes (e.g., group name, description).
    /// Allows libxmtp to receive attributes from updated versions not yet captured in MetadataField.
    pub attributes: HashMap<String, String>,
    /// List of admin inbox IDs for this group.
    /// See `GroupMutablePermissions` for more details on admin permissions.
    pub admin_list: Vec<String>,
    /// List of super admin inbox IDs for this group.
    /// See `GroupMutablePermissions` for more details on super admin permissions.
    pub super_admin_list: Vec<String>,
}

impl GroupMutableMetadata {
    /// Creates a new GroupMutableMetadata instance.
    pub fn new(
        attributes: HashMap<String, String>,
        admin_list: Vec<String>,
        super_admin_list: Vec<String>,
    ) -> Self {
        Self {
            attributes,
            admin_list,
            super_admin_list,
        }
    }

    /// Creates a new GroupMutableMetadata instance with default values.
    /// The creator is automatically added as a super admin.
    /// See `GroupMutablePermissions` for more details on super admin permissions.
    pub fn new_default(
        creator_inbox_id: String,
        commit_log_signer: Option<Secret>,
        opts: GroupMetadataOptions,
    ) -> Self {
        let mut attributes = HashMap::new();
        attributes.insert(
            MetadataField::GroupName.to_string(),
            opts.name.unwrap_or_else(|| DEFAULT_GROUP_NAME.to_string()),
        );
        attributes.insert(
            MetadataField::Description.to_string(),
            opts.description
                .unwrap_or_else(|| DEFAULT_GROUP_DESCRIPTION.to_string()),
        );
        attributes.insert(
            MetadataField::GroupImageUrlSquare.to_string(),
            opts.image_url_square
                .unwrap_or_else(|| DEFAULT_GROUP_IMAGE_URL_SQUARE.to_string()),
        );
        attributes.insert(
            MetadataField::AppData.to_string(),
            opts.app_data.unwrap_or_default(),
        );

        if let Some(message_disappearing_settings) = opts.message_disappearing_settings {
            attributes.insert(
                MetadataField::MessageDisappearFromNS.to_string(),
                message_disappearing_settings.from_ns.to_string(),
            );
            attributes.insert(
                MetadataField::MessageDisappearInNS.to_string(),
                message_disappearing_settings.in_ns.to_string(),
            );
        }

        if let Some(signer) = commit_log_signer {
            attributes.insert(
                MetadataField::CommitLogSigner.to_string(),
                hex::encode(signer.as_slice()),
            );
        }

        let admin_list = vec![];
        let super_admin_list = vec![creator_inbox_id.clone()];
        Self {
            attributes,
            admin_list,
            super_admin_list,
        }
    }

    // Admin / super admin is not needed for a DM
    pub fn new_dm_default(
        _creator_inbox_id: String,
        _dm_target_inbox_id: &str,
        commit_log_signer: Option<Secret>,
        opts: DMMetadataOptions,
    ) -> Self {
        let mut attributes = HashMap::new();
        // TODO: would it be helpful to incorporate the dm inbox ids in the name or description?
        attributes.insert(
            MetadataField::GroupName.to_string(),
            DEFAULT_GROUP_NAME.to_string(),
        );
        attributes.insert(
            MetadataField::Description.to_string(),
            DEFAULT_GROUP_DESCRIPTION.to_string(),
        );
        attributes.insert(
            MetadataField::GroupImageUrlSquare.to_string(),
            DEFAULT_GROUP_IMAGE_URL_SQUARE.to_string(),
        );
        if let Some(message_disappearing_settings) = opts.message_disappearing_settings {
            attributes.insert(
                MetadataField::MessageDisappearFromNS.to_string(),
                message_disappearing_settings.from_ns.to_string(),
            );
            attributes.insert(
                MetadataField::MessageDisappearInNS.to_string(),
                message_disappearing_settings.in_ns.to_string(),
            );
        }

        if let Some(signer) = commit_log_signer {
            attributes.insert(
                MetadataField::CommitLogSigner.to_string(),
                hex::encode(signer.as_slice()),
            );
        }

        let admin_list = vec![];
        let super_admin_list = vec![];
        Self {
            attributes,
            admin_list,
            super_admin_list,
        }
    }

    /// Returns a vector of supported metadata fields.
    ///
    /// These fields will receive default permission policies for new groups.
    pub fn supported_fields() -> Vec<MetadataField> {
        vec![
            MetadataField::GroupName,
            MetadataField::Description,
            MetadataField::GroupImageUrlSquare,
            MetadataField::MessageDisappearFromNS,
            MetadataField::MessageDisappearInNS,
            MetadataField::MinimumSupportedProtocolVersion,
            MetadataField::AppData,
        ]
    }

    /// Checks if the given inbox ID is an admin.
    pub fn is_admin(&self, inbox_id: &String) -> bool {
        self.admin_list.contains(inbox_id)
    }

    /// Checks if the given inbox ID is a super admin.
    pub fn is_super_admin(&self, inbox_id: &String) -> bool {
        self.super_admin_list.contains(inbox_id)
    }

    /// Retrieves the commit log signer secret from the metadata attributes.
    /// Returns None if the field is not present or if hex decoding fails.
    pub fn commit_log_signer(&self) -> Option<Secret> {
        self.attributes
            .get(&MetadataField::CommitLogSigner.to_string())
            .and_then(|hex_str| hex::decode(hex_str).ok())
            .map(Secret::new)
    }
}

impl TryFrom<GroupMutableMetadata> for Vec<u8> {
    type Error = GroupMutableMetadataError;

    /// Converts GroupMutableMetadata to a byte vector for storage as an MLS Unknown Group Context Extension.
    fn try_from(value: GroupMutableMetadata) -> Result<Self, Self::Error> {
        let mut buf = Vec::new();
        let proto_val = GroupMutableMetadataProto {
            attributes: value.attributes.clone(),
            admin_list: Some(InboxesProto {
                inbox_ids: value.admin_list,
            }),
            super_admin_list: Some(InboxesProto {
                inbox_ids: value.super_admin_list,
            }),
        };
        proto_val.encode(&mut buf)?;

        Ok(buf)
    }
}

impl TryFrom<&Vec<u8>> for GroupMutableMetadata {
    type Error = GroupMutableMetadataError;

    /// Converts a byte vector to GroupMutableMetadata.
    fn try_from(value: &Vec<u8>) -> Result<Self, Self::Error> {
        let proto_val = GroupMutableMetadataProto::decode(value.as_slice())?;
        Self::try_from(proto_val)
    }
}

impl TryFrom<GroupMutableMetadataProto> for GroupMutableMetadata {
    type Error = GroupMutableMetadataError;

    /// Converts a GroupMutableMetadataProto to GroupMutableMetadata.
    fn try_from(value: GroupMutableMetadataProto) -> Result<Self, Self::Error> {
        let admin_list = value
            .admin_list
            .ok_or(GroupMutableMetadataError::MissingMetadataField)?
            .inbox_ids;

        let super_admin_list = value
            .super_admin_list
            .ok_or(GroupMutableMetadataError::MissingMetadataField)?
            .inbox_ids;

        Ok(Self::new(
            value.attributes.clone(),
            admin_list,
            super_admin_list,
        ))
    }
}

impl TryFrom<&Extensions<GroupContext>> for GroupMutableMetadata {
    type Error = GroupMutableMetadataError;

    /// Attempts to extract GroupMutableMetadata from MLS Extensions.
    fn try_from(value: &Extensions<GroupContext>) -> Result<Self, Self::Error> {
        match find_mutable_metadata_extension(value) {
            Some(metadata) => GroupMutableMetadata::try_from(metadata),
            None => Err(GroupMutableMetadataError::MissingExtension),
        }
    }
}

impl TryFrom<&OpenMlsGroup> for GroupMutableMetadata {
    type Error = GroupMutableMetadataError;

    /// Attempts to extract GroupMutableMetadata from an OpenMlsGroup.
    fn try_from(group: &OpenMlsGroup) -> Result<Self, Self::Error> {
        let extensions = group.extensions();
        extensions.try_into()
    }
}

/// Finds the mutable metadata extension in the given MLS Extensions.
///
/// This function searches for an Unknown Extension with the
/// [MUTABLE_METADATA_EXTENSION_ID].
pub fn find_mutable_metadata_extension(extensions: &Extensions<GroupContext>) -> Option<&Vec<u8>> {
    extensions.iter().find_map(|extension| {
        if let Extension::Unknown(MUTABLE_METADATA_EXTENSION_ID, UnknownExtension(metadata)) =
            extension
        {
            return Some(metadata);
        }
        None
    })
}

/// Read `GroupMutableMetadata` from the **legacy** group-context
/// extension only.
///
/// Use only when the caller is certain the group is unmigrated — on
/// post-bootstrap groups the legacy extension is gone and this returns
/// [`GroupMutableMetadataError::MissingExtension`].
///
/// For capability-aware reads that handle both legacy and migrated
/// groups, use `extract_group_mutable_metadata_capability_aware` in
/// the `xmtp_mls` crate at
/// `xmtp_mls::groups::app_data::component_source`.
/// (`xmtp_mls_common` cannot rustdoc-link to it because the dependency
/// direction is one-way — this comment is the pointer.)
pub fn extract_legacy_group_mutable_metadata(
    group: &OpenMlsGroup,
) -> Result<GroupMutableMetadata, GroupMutableMetadataError> {
    extract_legacy_group_mutable_metadata_from_extensions(group.extensions())
}

/// Same as [`extract_legacy_group_mutable_metadata`], but reads directly from a
/// group's `GroupContext` extensions — no full `OpenMlsGroup` needed.
pub fn extract_legacy_group_mutable_metadata_from_extensions(
    extensions: &Extensions<GroupContext>,
) -> Result<GroupMutableMetadata, GroupMutableMetadataError> {
    find_mutable_metadata_extension(extensions)
        .ok_or(GroupMutableMetadataError::MissingExtension)?
        .try_into()
}

/// Single source of truth for the `MetadataField` ↔ `ComponentId`
/// bijection over the Bytes/String-typed mutable-metadata family. The
/// dict↔legacy merge below and the lookup helpers in
/// `xmtp_mls::groups::app_data::component_source` all derive from this
/// table.
pub const METADATA_FIELD_COMPONENT_MAP: &[(
    MetadataField,
    super::app_data::component_id::ComponentId,
)] = &[
    (
        MetadataField::GroupName,
        super::app_data::component_id::ComponentId::GROUP_NAME,
    ),
    (
        MetadataField::Description,
        super::app_data::component_id::ComponentId::GROUP_DESCRIPTION,
    ),
    (
        MetadataField::GroupImageUrlSquare,
        super::app_data::component_id::ComponentId::GROUP_IMAGE_URL,
    ),
    (
        MetadataField::MessageDisappearFromNS,
        super::app_data::component_id::ComponentId::MESSAGE_DISAPPEAR_FROM_NS,
    ),
    (
        MetadataField::MessageDisappearInNS,
        super::app_data::component_id::ComponentId::MESSAGE_DISAPPEAR_IN_NS,
    ),
    (
        MetadataField::MinimumSupportedProtocolVersion,
        super::app_data::component_id::ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION,
    ),
    (
        MetadataField::CommitLogSigner,
        super::app_data::component_id::ComponentId::COMMIT_LOG_SIGNER,
    ),
    (
        MetadataField::AppData,
        super::app_data::component_id::ComponentId::APP_DATA,
    ),
];

/// Production migration predicate over raw extensions: the group is
/// post-bootstrap iff the AppData dictionary carries the
/// `COMPONENT_REGISTRY` entry (the bootstrap commit's first write).
///
/// `xmtp_mls::groups::app_data::is_migrated_extensions` layers a
/// test-only registry override on top of this; use that one inside
/// `xmtp_mls`. This variant exists for crates below `xmtp_mls` in the
/// dependency graph (e.g. the archive exporter).
pub fn extensions_are_migrated(extensions: &Extensions<GroupContext>) -> bool {
    extensions
        .app_data_dictionary()
        .map(|ext| {
            ext.dictionary()
                .get(&super::app_data::component_id::ComponentId::COMPONENT_REGISTRY.as_u16())
                .is_some()
        })
        .unwrap_or(false)
}

/// Overlay the AppData dictionary's metadata components onto `base` —
/// the dict→legacy direction of the capability-aware read paths.
///
/// **Ungated**: callers decide migration state before calling (the
/// `xmtp_mls` wrapper gates on its test-override-aware
/// `is_migrated_extensions`; the archive exporter gates on
/// [`extensions_are_migrated`]). No-op when the extensions carry no
/// AppData dictionary.
///
/// Value translation per component family:
/// - `MESSAGE_DISAPPEAR_*`: 8-byte BE `i64` on the wire → base-10
///   string for the legacy reader.
/// - `COMMIT_LOG_SIGNER`: raw 32 key bytes → hex string.
/// - Every other metadata attribute: UTF-8 passthrough.
/// - `ADMIN_LIST` / `SUPER_ADMIN_LIST`: `TlsSet<InboxId>` → hex-string
///   lists (dict is authoritative on migrated groups).
pub fn merge_dict_into_mutable_metadata(
    base: &mut GroupMutableMetadata,
    extensions: &Extensions<GroupContext>,
) -> Result<(), GroupMutableMetadataError> {
    use super::app_data::component_id::ComponentId;

    let Some(ext) = extensions.app_data_dictionary() else {
        return Ok(());
    };
    let dict = ext.dictionary();

    for (field, id) in METADATA_FIELD_COMPONENT_MAP {
        if let Some(bytes) = dict.get(&id.as_u16()) {
            let legacy_value = decode_metadata_component(*id, bytes)?;
            base.attributes
                .insert(field.as_str().to_string(), legacy_value);
        }
    }

    for (component_id, list) in [
        (ComponentId::ADMIN_LIST, &mut base.admin_list),
        (ComponentId::SUPER_ADMIN_LIST, &mut base.super_admin_list),
    ] {
        if let Some(bytes) = dict.get(&component_id.as_u16()) {
            *list = decode_inbox_id_list(component_id, bytes)?;
        }
    }
    Ok(())
}

/// Best-effort variant of [`merge_dict_into_mutable_metadata`] that
/// degrades per-field instead of failing per-group: every component
/// that decodes is applied to `base`, every component that doesn't is
/// skipped, and the errors are returned so the caller can log them
/// (empty vec = clean merge).
///
/// Exists for the archive exporter, where one malformed component must
/// not drop the whole group from a backup — the group's messages are
/// exported unconditionally, so a missing group row orphans them and
/// aborts the entire restore on a foreign-key violation. Non-export
/// callers that want fail-fast semantics keep using the strict variant
/// above.
pub fn merge_dict_into_mutable_metadata_lossy(
    base: &mut GroupMutableMetadata,
    extensions: &Extensions<GroupContext>,
) -> Vec<GroupMutableMetadataError> {
    use super::app_data::component_id::ComponentId;

    let Some(ext) = extensions.app_data_dictionary() else {
        return Vec::new();
    };
    let dict = ext.dictionary();
    let mut errors = Vec::new();

    for (field, id) in METADATA_FIELD_COMPONENT_MAP {
        if let Some(bytes) = dict.get(&id.as_u16()) {
            match decode_metadata_component(*id, bytes) {
                Ok(legacy_value) => {
                    base.attributes
                        .insert(field.as_str().to_string(), legacy_value);
                }
                Err(e) => errors.push(e),
            }
        }
    }

    for (component_id, list) in [
        (ComponentId::ADMIN_LIST, &mut base.admin_list),
        (ComponentId::SUPER_ADMIN_LIST, &mut base.super_admin_list),
    ] {
        if let Some(bytes) = dict.get(&component_id.as_u16()) {
            match decode_inbox_id_list(component_id, bytes) {
                Ok(ids) => *list = ids,
                Err(e) => errors.push(e),
            }
        }
    }
    errors
}

/// Decode one Bytes/String-family component's wire bytes into its
/// legacy string value, per the translation rules documented on
/// [`merge_dict_into_mutable_metadata`]. Shared by the strict and
/// lossy merge variants so both apply identical translations.
fn decode_metadata_component(
    id: super::app_data::component_id::ComponentId,
    bytes: &[u8],
) -> Result<String, GroupMutableMetadataError> {
    use super::app_data::component_id::ComponentId;

    match id {
        ComponentId::MESSAGE_DISAPPEAR_FROM_NS | ComponentId::MESSAGE_DISAPPEAR_IN_NS => {
            let arr: [u8; 8] =
                bytes
                    .try_into()
                    .map_err(|_| GroupMutableMetadataError::MalformedComponent {
                        component_id: Some(id),
                        reason: format!("expected 8 bytes (BE i64), got {}", bytes.len()),
                    })?;
            Ok(i64::from_be_bytes(arr).to_string())
        }
        ComponentId::COMMIT_LOG_SIGNER => Ok(hex::encode(bytes)),
        _ => Ok(std::str::from_utf8(bytes)
            .map_err(|e| GroupMutableMetadataError::MalformedComponent {
                component_id: Some(id),
                reason: format!("non-UTF-8 bytes: {e}"),
            })?
            .to_string()),
    }
}

/// Decode an `ADMIN_LIST` / `SUPER_ADMIN_LIST` component's wire bytes
/// (`TlsSet<InboxId>`) into the legacy hex-string list form. Shared by
/// the strict and lossy merge variants.
fn decode_inbox_id_list(
    component_id: super::app_data::component_id::ComponentId,
    bytes: &[u8],
) -> Result<Vec<String>, GroupMutableMetadataError> {
    use super::inbox_id::InboxId;
    use super::tls_set::TlsSet;
    use tls_codec::Deserialize as _;

    let set = TlsSet::<InboxId>::tls_deserialize_exact(bytes).map_err(|e| {
        GroupMutableMetadataError::MalformedComponent {
            component_id: Some(component_id),
            reason: format!("invalid TlsSet<InboxId>: {e}"),
        }
    })?;
    Ok(set.iter().map(|id| id.to_hex()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_commit_log_signer_utility_method() {
        // Test with valid hex-encoded signer
        let test_secret_bytes = vec![1u8; 32];
        let test_secret_hex = hex::encode(&test_secret_bytes);

        let mut attributes = HashMap::new();
        attributes.insert(
            MetadataField::CommitLogSigner.to_string(),
            test_secret_hex.clone(),
        );

        let metadata = GroupMutableMetadata::new(attributes, vec![], vec![]);

        let retrieved_secret = metadata.commit_log_signer().unwrap();
        assert_eq!(retrieved_secret.as_slice(), &test_secret_bytes);

        // Test with missing signer
        let empty_metadata = GroupMutableMetadata::new(HashMap::new(), vec![], vec![]);
        assert!(empty_metadata.commit_log_signer().is_none());

        // Test with invalid hex
        let mut bad_attributes = HashMap::new();
        bad_attributes.insert(
            MetadataField::CommitLogSigner.to_string(),
            "invalid_hex".to_string(),
        );

        let bad_metadata = GroupMutableMetadata::new(bad_attributes, vec![], vec![]);
        assert!(bad_metadata.commit_log_signer().is_none());
    }

    #[xmtp_common::test]
    fn test_lossy_merge_applies_good_fields_and_reports_bad_ones() {
        use super::super::app_data::component_id::ComponentId;
        use openmls::extensions::{AppDataDictionary, AppDataDictionaryExtension};
        use openmls::group::GroupContext;

        // One valid component (GROUP_NAME), two malformed ones
        // (MESSAGE_DISAPPEAR_FROM_NS with the wrong byte width,
        // ADMIN_LIST with bytes that aren't a TlsSet<InboxId>).
        let mut dict = AppDataDictionary::new();
        let _ = dict.insert(ComponentId::GROUP_NAME.as_u16(), b"Good Name".to_vec());
        let _ = dict.insert(
            ComponentId::MESSAGE_DISAPPEAR_FROM_NS.as_u16(),
            vec![0x01; 3],
        );
        let _ = dict.insert(ComponentId::ADMIN_LIST.as_u16(), vec![0xff, 0xff, 0xff]);
        let extensions: Extensions<GroupContext> =
            Extensions::from_vec(vec![Extension::AppDataDictionary(
                AppDataDictionaryExtension::new(dict),
            )])
            .unwrap();

        // The strict variant fails on the first malformed component.
        let mut strict_base = GroupMutableMetadata::new(HashMap::new(), vec![], vec![]);
        assert!(merge_dict_into_mutable_metadata(&mut strict_base, &extensions).is_err());

        // The lossy variant applies the good field, leaves the bad
        // ones untouched, and reports both errors with their ids.
        let mut base = GroupMutableMetadata::new(HashMap::new(), vec![], vec![]);
        let errors = merge_dict_into_mutable_metadata_lossy(&mut base, &extensions);

        assert_eq!(
            base.attributes
                .get(MetadataField::GroupName.as_str())
                .map(String::as_str),
            Some("Good Name"),
        );
        assert!(
            !base
                .attributes
                .contains_key(MetadataField::MessageDisappearFromNS.as_str())
        );
        assert!(base.admin_list.is_empty());

        let error_ids: Vec<_> = errors
            .iter()
            .map(|e| match e {
                GroupMutableMetadataError::MalformedComponent { component_id, .. } => {
                    component_id.unwrap()
                }
                other => panic!("expected MalformedComponent, got: {other:?}"),
            })
            .collect();
        assert_eq!(
            error_ids,
            vec![
                ComponentId::MESSAGE_DISAPPEAR_FROM_NS,
                ComponentId::ADMIN_LIST
            ]
        );
    }
}
