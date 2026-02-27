//! commonly used type definitions specific to mls
use crate::context::XmtpMlsLocalContext;
use std::sync::Arc;
use xmtp_api::ApiDebugWrapper;
use xmtp_api_d14n::{ClientBundle, TrackedStatsClient};

pub type MlsContext =
    Arc<XmtpMlsLocalContext<WrappedXmtpApiClient, xmtp_db::DefaultStore, xmtp_db::DefaultMlsStore>>;

pub type WrappedXmtpApiClient = ApiDebugWrapper<TrackedStatsClient<XmtpApiClient>>;

pub type XmtpApiClient = xmtp_api_d14n::definitions::XmtpApiClient;

pub type XmtpClientBundle = ClientBundle;
