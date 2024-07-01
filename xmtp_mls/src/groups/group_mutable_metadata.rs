use std::{collections::HashMap, fmt};

use openmls::{
    extensions::{Extension, Extensions, UnknownExtension},
    group::MlsGroup as OpenMlsGroup,
};
use prost::Message;
use thiserror::Error;

use xmtp_proto::xmtp::mls::message_contents::{
    GroupMutableMetadataV1 as GroupMutableMetadataProto, Inboxes as InboxesProto,
};

use crate::configuration::{
    DEFAULT_GROUP_DESCRIPTION, DEFAULT_GROUP_IMAGE_URL_SQUARE, DEFAULT_GROUP_NAME,
    MUTABLE_METADATA_EXTENSION_ID,
};

use super::GroupMetadataOptions;

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
    #[error("metadata field is missing")]
    MissingMetadataField,
}

// Fields should be added to supported_fields fn for Metadata Update Support
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetadataField {
    GroupName,
    Description,
    GroupImageUrlSquare,
}

impl MetadataField {
    pub const fn as_str(&self) -> &'static str {
        match self {
            MetadataField::GroupName => "group_name",
            MetadataField::Description => "description",
            MetadataField::GroupImageUrlSquare => "group_image_url_square",
        }
    }
}

impl fmt::Display for MetadataField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GroupMutableMetadata {
    // Allow libxmtp to receive attributes from updated versions not yet captured in MetadataField
    pub attributes: HashMap<String, String>,
    pub admin_list: Vec<String>,
    pub super_admin_list: Vec<String>,
}

impl GroupMutableMetadata {
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

    pub fn new_default(creator_inbox_id: String, opts: GroupMetadataOptions) -> Self {
        let mut attributes = HashMap::new();
        attributes.insert(
            MetadataField::GroupName.to_string(),
            opts.name.unwrap_or_else(|| DEFAULT_GROUP_NAME.to_string()),
        );
        attributes.insert(
            MetadataField::Description.to_string(),
            opts.description.unwrap_or_else(|| DEFAULT_GROUP_DESCRIPTION.to_string()),
        );
        attributes.insert(
            MetadataField::GroupImageUrlSquare.to_string(),
            opts.image_url_square
                .unwrap_or_else(|| DEFAULT_GROUP_IMAGE_URL_SQUARE.to_string()),
        );
        let admin_list = vec![];
        let super_admin_list = vec![creator_inbox_id.clone()];
        Self {
            attributes,
            admin_list,
            super_admin_list,
        }
    }

    // These fields will receive default permission policies for new groups
    pub fn supported_fields() -> Vec<MetadataField> {
        vec![
            MetadataField::GroupName,
            MetadataField::Description,
            MetadataField::GroupImageUrlSquare,
        ]
    }

    pub fn is_admin(&self, inbox_id: &String) -> bool {
        self.admin_list.contains(inbox_id)
    }

    pub fn is_super_admin(&self, inbox_id: &String) -> bool {
        self.super_admin_list.contains(inbox_id)
    }
}

impl TryFrom<GroupMutableMetadata> for Vec<u8> {
    type Error = GroupMutableMetadataError;

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

    fn try_from(value: &Vec<u8>) -> Result<Self, Self::Error> {
        let proto_val = GroupMutableMetadataProto::decode(value.as_slice())?;
        Self::try_from(proto_val)
    }
}

impl TryFrom<GroupMutableMetadataProto> for GroupMutableMetadata {
    type Error = GroupMutableMetadataError;

    fn try_from(value: GroupMutableMetadataProto) -> Result<Self, Self::Error> {
        let admin_list = value
            .admin_list
            .ok_or_else(|| GroupMutableMetadataError::MissingMetadataField)?
            .inbox_ids;

        let super_admin_list = value
            .super_admin_list
            .ok_or_else(|| GroupMutableMetadataError::MissingMetadataField)?
            .inbox_ids;

        Ok(Self::new(
            value.attributes.clone(),
            admin_list,
            super_admin_list,
        ))
    }
}

impl TryFrom<&Extensions> for GroupMutableMetadata {
    type Error = GroupMutableMetadataError;

    fn try_from(value: &Extensions) -> Result<Self, Self::Error> {
        match find_mutable_metadata_extension(value) {
            Some(metadata) => GroupMutableMetadata::try_from(metadata),
            None => Err(GroupMutableMetadataError::MissingExtension),
        }
    }
}

impl TryFrom<&OpenMlsGroup> for GroupMutableMetadata {
    type Error = GroupMutableMetadataError;

    fn try_from(value: &OpenMlsGroup) -> Result<Self, Self::Error> {
        let extensions = value.export_group_context().extensions();
        extensions.try_into()
    }
}

pub fn find_mutable_metadata_extension(extensions: &Extensions) -> Option<&Vec<u8>> {
    extensions.iter().find_map(|extension| {
        if let Extension::Unknown(MUTABLE_METADATA_EXTENSION_ID, UnknownExtension(metadata)) =
            extension
        {
            return Some(metadata);
        }
        None
    })
}
