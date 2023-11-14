use crate::utils::id::serialize_group_id;

pub fn get_group_topic(group_id: &[u8]) -> String {
    format!("/xmtp/3/g-{}/proto", serialize_group_id(group_id))
}

pub fn get_welcome_topic(installation_id: &Vec<u8>) -> String {
    format!("/xmtp/3/w-{}/proto", hex::encode(installation_id))
}
