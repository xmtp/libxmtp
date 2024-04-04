use openmls::{
    extensions::{Extension, UnknownExtension},
    group::MlsGroup as OpenMlsGroup,
};
use prost::Message;
use thiserror::Error;

use xmtp_proto::xmtp::mls::message_contents::GroupMutableMetadataV1 as GroupMutableMetadataProto;

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
    pub allow_list_account_addresses: Vec<String>,
}

impl GroupMutableMetadata {
    pub fn new(group_name: String, allow_list_account_addresses: Vec<String>) -> Self {
        Self {
            group_name,
            allow_list_account_addresses,
        }
    }

    pub(crate) fn from_proto(
        proto: GroupMutableMetadataProto,
    ) -> Result<Self, GroupMutableMetadataError> {
        Ok(Self::new(
            proto.group_name.clone(),
            proto.allow_list_account_addresses.clone(),
        ))
    }

    pub(crate) fn to_proto(&self) -> Result<GroupMutableMetadataProto, GroupMutableMetadataError> {
        Ok(GroupMutableMetadataProto {
            group_name: self.group_name.clone(),
            allow_list_account_addresses: self.allow_list_account_addresses.clone(),
        })
    }
}

impl TryFrom<GroupMutableMetadata> for Vec<u8> {
    type Error = GroupMutableMetadataError;

    fn try_from(value: GroupMutableMetadata) -> Result<Self, Self::Error> {
        let mut buf = Vec::new();
        let proto_val = value.to_proto()?;
        proto_val.encode(&mut buf)?;

        Ok(buf)
    }
}

impl TryFrom<&Vec<u8>> for GroupMutableMetadata {
    type Error = GroupMutableMetadataError;

    fn try_from(value: &Vec<u8>) -> Result<Self, Self::Error> {
        let proto_val = GroupMutableMetadataProto::decode(value.as_slice())?;
        Self::from_proto(proto_val)
    }
}

impl TryFrom<GroupMutableMetadataProto> for GroupMutableMetadata {
    type Error = GroupMutableMetadataError;

    fn try_from(value: GroupMutableMetadataProto) -> Result<Self, Self::Error> {
        Self::from_proto(value)
    }
}

pub fn extract_group_mutable_metadata(
    group: &OpenMlsGroup,
) -> Result<GroupMutableMetadata, GroupMutableMetadataError> {
    let extensions = group.export_group_context().extensions();
    for extension in extensions.iter() {
        // TODO: Replace with Constant
        if let Extension::Unknown(0xff11, UnknownExtension(meta_data)) = extension {
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
