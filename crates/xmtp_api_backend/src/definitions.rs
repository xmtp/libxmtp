use crate::{BackendClient, TrackedStatsClient};
use std::sync::Arc;
use xmtp_proto::api::ArcClient;
pub type ApiClient = BackendClient<ArcClient>;
pub type XmtpApiClient = Arc<TrackedStatsClient<ApiClient>>;
