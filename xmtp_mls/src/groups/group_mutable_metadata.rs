use std::{collections::HashMap, fmt};

use openmls::{
    extensions::{Extension, Extensions, UnknownExtension},
    group::MlsGroup as OpenMlsGroup,
};
use prost::Message;
use thiserror::Error;

use xmtp_proto::xmtp::mls::message_contents::GroupMutableMetadataV1 as GroupMutableMetadataProto;

use crate::configuration::{
    DEFAULT_GROUP_DESCRIPTION, DEFAULT_GROUP_NAME, MUTABLE_METADATA_EXTENSION_ID,
};

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
}

// Fields should be added to supported_fields fn for Metadata Update Support
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetadataField {
    GroupName,
    Description,
}

impl MetadataField {
    fn as_str(self) -> &'static str {
        match self {
            MetadataField::GroupName => "group_name",
            MetadataField::Description => "description",
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
}

impl Default for GroupMutableMetadata {
    fn default() -> Self {
        let mut attributes = HashMap::new();
        attributes.insert(
            MetadataField::GroupName.to_string(),
            DEFAULT_GROUP_NAME.to_string(),
        );
        attributes.insert(
            MetadataField::Description.to_string(),
            DEFAULT_GROUP_DESCRIPTION.to_string(),
        );
        Self { attributes }
    }
}

impl GroupMutableMetadata {
    pub fn new(attributes: HashMap<String, String>) -> Self {
        Self { attributes }
    }

    // These fields will receive default permission policies for new groups
    pub fn supported_fields() -> Vec<MetadataField> {
        vec![MetadataField::GroupName, MetadataField::Description]
    }
}

impl TryFrom<GroupMutableMetadata> for Vec<u8> {
    type Error = GroupMutableMetadataError;

    fn try_from(value: GroupMutableMetadata) -> Result<Self, Self::Error> {
        let mut buf = Vec::new();
        let proto_val = GroupMutableMetadataProto {
            attributes: value.attributes.clone(),
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
        Ok(Self::new(value.attributes.clone()))
    }
}

impl TryFrom<&Extensions> for GroupMutableMetadata {
    type Error = GroupMutableMetadataError;

    fn try_from(value: &Extensions) -> Result<Self, Self::Error> {
        for extension in value.iter() {
            if let Extension::Unknown(MUTABLE_METADATA_EXTENSION_ID, UnknownExtension(metadata)) =
                extension
            {
                return GroupMutableMetadata::try_from(metadata);
            }
        }
        Err(GroupMutableMetadataError::MissingExtension)
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

pub fn extract_group_mutable_metadata(
    group: &OpenMlsGroup,
) -> Result<GroupMutableMetadata, GroupMutableMetadataError> {
    let extensions = group.export_group_context().extensions();
    extensions.try_into()
}
