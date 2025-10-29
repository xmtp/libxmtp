//! commonly used type definitions specific to mls
use crate::context::XmtpMlsLocalContext;
use std::sync::Arc;
use xmtp_api::ApiDebugWrapper;
use xmtp_api_d14n::TrackedStatsClient;
use xmtp_api_d14n::protocol::FullXmtpApiArc;

pub type MlsContext =
    Arc<XmtpMlsLocalContext<WrappedXmtpApiClient, xmtp_db::DefaultStore, xmtp_db::DefaultMlsStore>>;

pub type WrappedXmtpApiClient = ApiDebugWrapper<TrackedStatsClient<FullXmtpApiArc>>;

pub type XmtpApiClient = FullXmtpApiArc;
