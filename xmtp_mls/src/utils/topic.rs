pub fn get_group_topic(group_id: &Vec<u8>) -> String {
    format!("/xmtp/3/g-{}/proto", serialize_group_id(group_id))
}

pub fn serialize_group_id(group_id: &[u8]) -> String {
    // TODO: I wonder if we really want to be base64 encoding this or if we can treat it as a
    // slice
    hex::encode(group_id)
}

pub fn get_welcome_topic(installation_id: &Vec<u8>) -> String {
    format!("/xmtp/3/w-{}/proto", hex::encode(installation_id))
}
