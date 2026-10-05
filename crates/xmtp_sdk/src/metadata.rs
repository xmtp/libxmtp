//! Typed metadata fields and user profiles.
//!
//! A component ID identifies a field. Its name is a label from the protocol
//! or the backend catalogue, and the key `metadata_field` looks up. Two refs
//! with one ID and different names name the same field, so compare
//! `component_id`s rather than whole refs.

#[cfg(not(feature = "pure-only"))]
mod convert;
#[cfg(not(feature = "pure-only"))]
pub(crate) use convert::core_inbox_id;
#[cfg(all(
    any(all(test, not(target_arch = "wasm32")), feature = "conformance"),
    not(feature = "pure-only")
))]
pub(crate) mod catalogue_override;

use xmtp_mls::mls_common::app_data::fields::MetadataFieldRef as CoreFieldRef;

/// Names a metadata field: its component ID and, when one exists, its
/// well-known or backend catalogue name.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MetadataFieldRef {
    pub component_id: u16,
    pub name: Option<String>,
}

impl From<CoreFieldRef> for MetadataFieldRef {
    fn from(value: CoreFieldRef) -> Self {
        Self {
            component_id: value.component_id.as_u16(),
            name: value.name.map(|name| name.into_owned()),
        }
    }
}

/// A public well-known metadata field.
#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum WellKnownMetadataField {
    GroupName,
    GroupDescription,
    GroupImageUrl,
    /// Opaque image bytes. The app uploads, downloads and falls back.
    GroupImage,
    AppData,
    UserDisplayName,
    MessageDisappearFromNs,
    MessageDisappearInNs,
}

/// The ref of a well-known field, named with its protocol name.
#[xmtp_macro::sdk_export(pure)]
pub fn metadata_field_ref(field: WellKnownMetadataField) -> MetadataFieldRef {
    use WellKnownMetadataField as F;
    match field {
        F::GroupName => CoreFieldRef::GROUP_NAME,
        F::GroupDescription => CoreFieldRef::GROUP_DESCRIPTION,
        F::GroupImageUrl => CoreFieldRef::GROUP_IMAGE_URL,
        F::GroupImage => CoreFieldRef::GROUP_IMAGE,
        F::AppData => CoreFieldRef::APP_DATA,
        F::UserDisplayName => CoreFieldRef::USER_DISPLAY_NAME,
        F::MessageDisappearFromNs => CoreFieldRef::MESSAGE_DISAPPEAR_FROM_NS,
        F::MessageDisappearInNs => CoreFieldRef::MESSAGE_DISAPPEAR_IN_NS,
    }
    .into()
}

#[cfg(not(feature = "pure-only"))]
include!("metadata/records.rs");

#[cfg(test)]
mod tests {
    use super::{WellKnownMetadataField as F, metadata_field_ref};

    /// Each well-known ref carries the protocol ID and name.
    #[xmtp_common::test(unwrap_try = true)]
    fn well_known_refs_have_protocol_ids() {
        let refs: Vec<_> = [
            F::GroupName,
            F::GroupDescription,
            F::GroupImageUrl,
            F::MessageDisappearFromNs,
            F::MessageDisappearInNs,
            F::AppData,
            F::UserDisplayName,
            F::GroupImage,
        ]
        .into_iter()
        .map(metadata_field_ref)
        .map(|field| (field.component_id, field.name))
        .collect();
        assert_eq!(
            refs,
            [
                (0x8004, "GROUP_NAME"),
                (0x8005, "GROUP_DESCRIPTION"),
                (0x8006, "GROUP_IMAGE_URL"),
                (0x8007, "MESSAGE_DISAPPEAR_FROM_NS"),
                (0x8008, "MESSAGE_DISAPPEAR_IN_NS"),
                (0x8009, "APP_DATA"),
                (0x800C, "USER_DISPLAY_NAME"),
                (0x800D, "GROUP_IMAGE"),
            ]
            .map(|(id, name)| (id, Some(name.to_owned())))
        );
    }
}
