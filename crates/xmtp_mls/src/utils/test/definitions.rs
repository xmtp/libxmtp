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

/// A test client whose API is the v3↔d14n [`MigrationClient`](xmtp_api_d14n::MigrationClient)
/// — the coexistence object production ships. Unlike [`TestClient`], its backend is not
/// feature-switched: it always holds both, and a test drives the cutover via the cursor
/// store. Used by the live coexistence test to prove messaging works through the real
/// migration client.
pub type MigrationTestClient = xmtp_api_d14n::TestMigrationClient;
pub type MigrationXmtpMlsContext =
    Arc<XmtpMlsLocalContext<MigrationTestClient, xmtp_db::DefaultStore, TestMlsStorage>>;
pub type MigrationXmtpClient = Client<MigrationXmtpMlsContext>;

/// A Test client
/// This client switches its backend based on feature flag.
/// default: V3 , Local
/// --feature d14n: d14n, Local
/// --feature d14n --feature dev: d14n, devnet
/// -- features dev: v3, local
pub type TestClient = xmtp_api_d14n::TestClient;

/// Test client that is local only, but still switches between d14n/v3 clients on feature flag
pub type LocalOnlyTestClientCreator = xmtp_api_d14n::LocalOnlyTestClientCreator;

/// Local-only creator for the v3↔d14n `MigrationClient` (does not feature-switch).
pub type LocalOnlyMigrationClientCreator = xmtp_api_d14n::LocalOnlyMigrationClientCreator;

/// Test client that is dev only, but still switches between d14n/v3 clients on feature flag
pub type DevOnlyTestClientCreator = xmtp_api_d14n::DevOnlyTestClientCreator;

/// Test client builder for toxics
pub type ToxicOnlyTestClientCreator = xmtp_api_d14n::ToxicOnlyTestClientCreator;

/// a v3/d14n, local/dev client that switches based on feature flag
pub type FeatureSwitchedTestClientCreator = xmtp_api_d14n::FeatureSwitchedTestClientCreator;

pub type DefaultTestClientCreator = xmtp_api_d14n::FeatureSwitchedTestClientCreator;
