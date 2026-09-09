//! commonly used type definitions specific to mls
use crate::context::XmtpMlsLocalContext;
use std::sync::Arc;

pub type MlsContext =
    Arc<XmtpMlsLocalContext<WrappedXmtpApiClient, xmtp_db::DefaultStore, xmtp_db::DefaultMlsStore>>;

pub type WrappedXmtpApiClient = XmtpApiClient;

pub type XmtpApiClient = xmtp_api_backend::XmtpApiClient;
