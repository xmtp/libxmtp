use std::sync::Arc;

use alloy::signers::local::PrivateKeySigner;
use xmtp_db::sql_key_store::SqlKeyStore;

use crate::{Client, context::XmtpMlsLocalContext, utils::Tester};

pub type TestMlsStorage = SqlKeyStore<xmtp_db::DefaultDbConnection>;
pub type TestXmtpMlsContext =
    Arc<XmtpMlsLocalContext<TestClient, xmtp_db::DefaultStore, TestMlsStorage>>;
pub type FullXmtpClient = Client<TestXmtpMlsContext>;
/// Default Client Tester type
pub type ClientTester = Tester<PrivateKeySigner, FullXmtpClient>;
pub type TestMlsGroup = crate::groups::MlsGroup<TestXmtpMlsContext>;

/// One backend API client. Clones retain its wire identity.
pub type TestClient = Arc<xmtp_api_d14n::TestClient>;

pub type DefaultTestClientCreator = xmtp_api_d14n::TestClient;
pub type ToxicOnlyTestClientCreator = xmtp_api_d14n::ToxicTestClientCreator;
