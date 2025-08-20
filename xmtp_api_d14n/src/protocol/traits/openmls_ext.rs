use openmls::prelude::{ContentType, ProtocolMessage};
use xmtp_proto::types::GroupMessage;
/// Decentralization specific d14n Extension trait for MLS
pub trait D14nMlsExt {
    /// attempt to pull out the constant v3 originator id from a message
    fn originator_id_v3(&self) -> u16;
}

impl D14nMlsExt for ProtocolMessage {
    fn originator_id_v3(&self) -> u16 {
        if self.content_type() == ContentType::Commit {
            xmtp_configuration::Originators::MLS_COMMITS
        } else {
            xmtp_configuration::Originators::APPLICATION_MESSAGES
        }
    }
}

impl D14nMlsExt for GroupMessage {
    fn originator_id_v3(&self) -> u16 {
        self.message.originator_id_v3()
    }
}
