use prost::{bytes::Bytes, Message};

use super::IntentError;
use xmtp_proto::xmtp::mls::database::{
    update_metadata_data::{Version as UpdateMetadataVersion, V1 as UpdateMetadataV1},
    UpdateMetadataData,
};

use crate::groups::group_mutable_metadata::MetadataField;

#[derive(Debug, Clone)]
pub struct UpdateMetadataIntentData {
    pub field_name: String,
    pub field_value: String,
}

impl UpdateMetadataIntentData {
    pub fn new(field_name: String, field_value: String) -> Self {
        Self {
            field_name,
            field_value,
        }
    }

    pub fn new_update_group_name(group_name: String) -> Self {
        Self {
            field_name: MetadataField::GroupName.to_string(),
            field_value: group_name,
        }
    }

    pub fn new_update_group_image_url_square(group_image_url_square: String) -> Self {
        Self {
            field_name: MetadataField::GroupImageUrlSquare.to_string(),
            field_value: group_image_url_square,
        }
    }

    pub fn new_update_group_description(group_description: String) -> Self {
        Self {
            field_name: MetadataField::Description.to_string(),
            field_value: group_description,
        }
    }

    pub fn new_update_conversation_message_disappear_from_ns(from_ns: i64) -> Self {
        Self {
            field_name: MetadataField::MessageDisappearFromNS.to_string(),
            field_value: from_ns.to_string(),
        }
    }
    pub fn new_update_conversation_message_disappear_in_ns(in_ns: i64) -> Self {
        Self {
            field_name: MetadataField::MessageDisappearInNS.to_string(),
            field_value: in_ns.to_string(),
        }
    }

    pub fn new_update_group_min_version_to_match_self(min_version: String) -> Self {
        Self {
            field_name: MetadataField::MinimumSupportedProtocolVersion.to_string(),
            field_value: min_version,
        }
    }
}

impl From<UpdateMetadataIntentData> for Vec<u8> {
    fn from(intent: UpdateMetadataIntentData) -> Self {
        let mut buf = Vec::new();

        UpdateMetadataData {
            version: Some(UpdateMetadataVersion::V1(UpdateMetadataV1 {
                field_name: intent.field_name.to_string(),
                field_value: intent.field_value.clone(),
            })),
        }
        .encode(&mut buf)
        .expect("encode error");

        buf
    }
}

impl TryFrom<Vec<u8>> for UpdateMetadataIntentData {
    type Error = IntentError;

    fn try_from(data: Vec<u8>) -> Result<Self, Self::Error> {
        let msg = UpdateMetadataData::decode(Bytes::from(data))?;

        let field_name = match msg.version {
            Some(UpdateMetadataVersion::V1(ref v1)) => v1.field_name.clone(),
            None => return Err(IntentError::MissingPayload),
        };
        let field_value = match msg.version {
            Some(UpdateMetadataVersion::V1(ref v1)) => v1.field_value.clone(),
            None => return Err(IntentError::MissingPayload),
        };

        Ok(Self::new(field_name, field_value))
    }
}
