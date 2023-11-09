pub fn get_group_topic(group_id: &Vec<u8>) -> String {
    format!("/xmtp/3/g-{}/proto", hex::encode(group_id))
}

pub fn get_welcome_topic(installation_id: &Vec<u8>) -> String {
    format!("/xmtp/3/w-{}/proto", hex::encode(installation_id))
}
