//! commonly used type definitions specific to mls
#[cfg(feature = "sync")]
use crate::context::XmtpMlsLocalContext;
#[cfg(feature = "sync")]
use std::sync::Arc;
use xmtp_api_d14n::{ClientBundle, TrackedStatsClient};

// Built on the sync-track default store/key-store; the async track supplies its
// own store types (e.g. over `PgDb`/`PgKeyStore`), so this convenience alias is
// sync only.
#[cfg(feature = "sync")]
pub type MlsContext =
    Arc<XmtpMlsLocalContext<WrappedXmtpApiClient, xmtp_db::DefaultStore, xmtp_db::DefaultMlsStore>>;

pub type WrappedXmtpApiClient = TrackedStatsClient<XmtpApiClient>;

pub type XmtpApiClient = xmtp_api_d14n::definitions::XmtpApiClient;

pub type XmtpClientBundle = ClientBundle;
