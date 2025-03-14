pub struct ApiUrls;

impl ApiUrls {
    pub const LOCAL_ADDRESS: &'static str = "http://localhost:5555";
    pub const LOCAL_D14N_ADDRESS: &'static str = "http://localhost:5055";
    pub const DEV_ADDRESS: &'static str = "https://dev.xmtp.network:443";
    pub const PRODUCTION_ADDRESS: &'static str = "https://production.xmtp.network";
}

pub struct ApiEndpoints;

impl ApiEndpoints {
    pub const FETCH_KEY_PACKAGES: &'static str = "/mls/v1/fetch-key-packages";
    pub const GET_IDENTITY_UPDATES: &'static str = "/identity/v1/get-identity-updates";
    pub const GET_INBOX_IDS: &'static str = "/identity/v1/get-inbox-ids";
    pub const PUBLISH_IDENTITY_UPDATE: &'static str = "/identity/v1/publish-identity-update";
    pub const VERIFY_SMART_CONTRACT_WALLET_SIGNATURES: &'static str =
        "/identity/v1/verify-smart-contract-wallet-signatures";
    pub const QUERY_GROUP_MESSAGES: &'static str = "/mls/v1/query-group-messages";
    pub const QUERY_WELCOME_MESSAGES: &'static str = "/mls/v1/query-welcome-messages";
    pub const REGISTER_INSTALLATION: &'static str = "/mls/v1/register-installation";
    pub const SEND_GROUP_MESSAGES: &'static str = "/mls/v1/send-group-messages";
    pub const SEND_WELCOME_MESSAGES: &'static str = "/mls/v1/send-welcome-messages";
    pub const SUBSCRIBE_GROUP_MESSAGES: &'static str = "/mls/v1/subscribe-group-messages";
    pub const SUBSCRIBE_WELCOME_MESSAGES: &'static str = "/mls/v1/subscribe-welcome-messages";
    pub const UPLOAD_KEY_PACKAGE: &'static str = "/mls/v1/upload-key-package";
}
