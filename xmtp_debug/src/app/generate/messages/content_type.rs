use std::collections::HashMap;

use prost::Message;
use xmtp_proto::xmtp::message_contents::{ContentTypeId, EncodedContent};

/// Create a new message according to the xmtp content type
pub fn new_message(msg: String) -> Vec<u8> {
    let id = ContentTypeId {
        authority_id: "xmtp.org".into(),
        type_id: "text".into(),
        version_major: 1,
        version_minor: 0,
    };
    let content = EncodedContent {
        r#type: Some(id),
        parameters: vec![("encoding".to_string(), "UTF-8".to_string())]
            .into_iter()
            .collect::<HashMap<_, _>>(),
        fallback: None,
        compression: None,
        content: msg.as_bytes().to_vec(),
    };
    content.encode_to_vec()
}
