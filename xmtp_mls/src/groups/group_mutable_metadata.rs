use openmls::{
    extensions::{Extension, UnknownExtension},
    group::MlsGroup as OpenMlsGroup,
};
use prost::Message;
use thiserror::Error;

use xmtp_proto::xmtp::mls::message_contents::GroupMutableMetadataV1 as GroupMutableMetadataProto;

use crate::configuration::MUTABLE_METADATA_EXTENSION_ID;

#[derive(Debug, Error)]
pub enum GroupMutableMetadataError {
    #[error("serialization: {0}")]
    Serialization(#[from] prost::EncodeError),
    #[error("deserialization: {0}")]
    Deserialization(#[from] prost::DecodeError),
    #[error("missing extension")]
    MissingExtension,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GroupMutableMetadata {
    pub group_name: String,
}

impl GroupMutableMetadata {
    pub fn new(group_name: String) -> Self {
        Self { group_name }
    }
}

impl TryFrom<GroupMutableMetadata> for Vec<u8> {
    type Error = GroupMutableMetadataError;

    fn try_from(value: GroupMutableMetadata) -> Result<Self, Self::Error> {
        let mut buf = Vec::new();
        let proto_val = GroupMutableMetadataProto {
            group_name: value.group_name.clone(),
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
        Ok(Self::new(value.group_name.clone()))
    }
}

pub fn extract_group_mutable_metadata(
    group: &OpenMlsGroup,
) -> Result<GroupMutableMetadata, GroupMutableMetadataError> {
    let extensions = group.export_group_context().extensions();
    for extension in extensions.iter() {
        if let Extension::Unknown(MUTABLE_METADATA_EXTENSION_ID, UnknownExtension(meta_data)) =
            extension
        {
            return GroupMutableMetadata::try_from(meta_data);
        }
    }
    Err(GroupMutableMetadataError::MissingExtension)
}

#[cfg(test)]
mod tests {

    use super::*;
    #[test]
    fn test_preconfigured_mutable_metadata() {
        // TODO add test here
    }
}
