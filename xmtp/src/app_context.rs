use async_trait::async_trait;
use xmtp_proto::xmtp::message_api::v1::QueryRequest;

use crate::{
    account::Account, client::ClientError, contact::Contact, storage::EncryptedMessageStore,
    types::networking::XmtpApiClient, utils::build_user_contact_topic, Network,
};

pub struct AppContext<A>
where
    A: XmtpApiClient,
{
    pub(crate) api_client: A,
    pub(crate) network: Network,
    pub(crate) account: Account,
    pub store: EncryptedMessageStore, // Temporarily exposed outside crate for CLI client
}
