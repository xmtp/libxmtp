use std::collections::HashMap;

pub struct InvitationContext {
    conversation_id: String,
    metadata: HashMap<String, String>,
}

pub struct TopicData {
    key: Vec<u8>,
    context: Option<InvitationContext>,
    // timestamp in UTC
    created: u64,
}
