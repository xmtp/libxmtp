use super::IntentError;
use prost::{bytes::Bytes, Message};
use xmtp_proto::xmtp::mls::database::{
    update_admin_lists_data::{Version as UpdateAdminListsVersion, V1 as UpdateAdminListsV1},
    UpdateAdminListsData,
};

#[repr(i32)]
#[derive(Debug, Clone, PartialEq)]
pub enum AdminListActionType {
    Add = 1,         // Matches ADD_ADMIN in Protobuf
    Remove = 2,      // Matches REMOVE_ADMIN in Protobuf
    AddSuper = 3,    // Matches ADD_SUPER_ADMIN in Protobuf
    RemoveSuper = 4, // Matches REMOVE_SUPER_ADMIN in Protobuf
}

impl TryFrom<i32> for AdminListActionType {
    type Error = IntentError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(AdminListActionType::Add),
            2 => Ok(AdminListActionType::Remove),
            3 => Ok(AdminListActionType::AddSuper),
            4 => Ok(AdminListActionType::RemoveSuper),
            _ => Err(IntentError::UnknownAdminListAction),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UpdateAdminListIntentData {
    pub action_type: AdminListActionType,
    pub inbox_id: String,
}

impl UpdateAdminListIntentData {
    pub fn new(action_type: AdminListActionType, inbox_id: String) -> Self {
        Self {
            action_type,
            inbox_id,
        }
    }
}

impl From<UpdateAdminListIntentData> for Vec<u8> {
    fn from(intent: UpdateAdminListIntentData) -> Self {
        let mut buf = Vec::new();
        let action_type = intent.action_type as i32;

        UpdateAdminListsData {
            version: Some(UpdateAdminListsVersion::V1(UpdateAdminListsV1 {
                admin_list_update_type: action_type,
                inbox_id: intent.inbox_id,
            })),
        }
        .encode(&mut buf)
        .expect("encode error");

        buf
    }
}

impl TryFrom<Vec<u8>> for UpdateAdminListIntentData {
    type Error = IntentError;

    fn try_from(data: Vec<u8>) -> Result<Self, Self::Error> {
        let msg = UpdateAdminListsData::decode(Bytes::from(data))?;

        let action_type: AdminListActionType = match msg.version {
            Some(UpdateAdminListsVersion::V1(ref v1)) => {
                AdminListActionType::try_from(v1.admin_list_update_type)?
            }
            None => return Err(IntentError::MissingUpdateAdminVersion),
        };
        let inbox_id = match msg.version {
            Some(UpdateAdminListsVersion::V1(ref v1)) => v1.inbox_id.clone(),
            None => return Err(IntentError::MissingUpdateAdminVersion),
        };

        Ok(Self::new(action_type, inbox_id))
    }
}
