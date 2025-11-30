//! commonly used type definitions specific to mls
use crate::context::XmtpMlsLocalContext;
use std::sync::Arc;
use xmtp_api::ApiDebugWrapper;
use xmtp_api_d14n::protocol::FullXmtpApiArc;
use xmtp_api_d14n::{ClientBundle, TrackedStatsClient};
use xmtp_api_grpc::error::GrpcError;
use xmtp_proto::api::ApiClientError;

pub type MlsContext =
    Arc<XmtpMlsLocalContext<WrappedXmtpApiClient, xmtp_db::DefaultStore, xmtp_db::DefaultMlsStore>>;

pub type WrappedXmtpApiClient = ApiDebugWrapper<TrackedStatsClient<XmtpApiClient>>;

pub type XmtpApiClient = FullXmtpApiArc<ApiClientError<GrpcError>>;

pub type XmtpClientBundle = ClientBundle<GrpcError>;
