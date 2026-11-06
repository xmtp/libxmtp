use std::sync::Arc;

use crate::context::XmtpMlsLocalContext;
use crate::groups::MlsGroup;
use crate::groups::summary::SyncSummary;
use mockall::mock;
use xmtp_api::ApiClientWrapper;
use xmtp_api_backend::MockBackendClient;
use xmtp_db::sql_key_store::mock::MockSqlKeyStore;

mod generate;
pub use generate::*;
mod openmls_mock;
pub use openmls_mock::*;

pub type MockApiWrapper = Arc<ApiClientWrapper<Arc<MockBackendClient>>>;
pub type MockStoreAndContext =
    XmtpMlsLocalContext<Arc<MockBackendClient>, xmtp_db::MockXmtpDb, MockSqlKeyStore>;
pub type MockContext = Arc<
    XmtpMlsLocalContext<
        Arc<MockBackendClient>,
        xmtp_db::MockXmtpDb,
        xmtp_db::test_utils::MlsMemoryStorage,
    >,
>;
/// A mock context type that hasn't yet been added into an Arc type.
pub type NewMockContext = XmtpMlsLocalContext<
    Arc<MockBackendClient>,
    xmtp_db::MockXmtpDb,
    xmtp_db::test_utils::MlsMemoryStorage,
>;
pub type MockMlsGroup = MlsGroup<MockContext>;

mock! {
    pub MockMlsGroup {
        fn sync_with_conn(&self) -> Result<SyncSummary, SyncSummary>;
    }
}
