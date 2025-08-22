use chrono::Local;

use crate::{
    types::{Cursor, InstallationId},
    xmtp::mls::message_contents::WelcomeWrapperAlgorithm,
};

/// Welcome Message from the network
pub struct WelcomeMessage {
    /// cursor of this message
    cursor: Cursor,
    /// server timestamp indicating when this message was created
    created_ns: chrono::DateTime<Local>,
    /// Installation key of user sending the welcome
    installation_key: InstallationId,
    /// welcome message payload
    data: Vec<u8>,
    /// HPKE Public Key
    hpke_public_key: Vec<u8>,
    /// Welcome Wrapper Algorithm
    wrapper_algorithm: WelcomeWrapperAlgorithm,
    /// Extra metadata attached to welcome
    welcome_metadata: Vec<u8>,
}
