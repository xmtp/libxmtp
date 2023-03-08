use std::collections::HashMap;

pub struct InvitationContext {
    pub conversation_id: String,
    pub metadata: HashMap<String, String>,
}

pub struct TopicData {
    pub key: Vec<u8>,
    pub context: Option<InvitationContext>,
    pub peer_address: String,
    // timestamp in UTC
    pub created: u64,
}
