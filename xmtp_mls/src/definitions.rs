//! commonly used type definitions specific to mls
use crate::context::XmtpMlsLocalContext;
use std::sync::Arc;
use xmtp_api::ApiDebugWrapper;
use xmtp_api_d14n::TrackedStatsClient;
use xmtp_api_grpc::error::GrpcError;
use xmtp_proto::{api::ApiClientError, api_client::ArcedXmtpApiWithStreams};

pub type MlsContext = Arc<
    XmtpMlsLocalContext<
        ApiDebugWrapper<TrackedStatsClient<ArcedXmtpApiWithStreams<ApiClientError<GrpcError>>>>,
        xmtp_db::DefaultStore,
        xmtp_db::DefaultMlsStore,
    >,
>;
