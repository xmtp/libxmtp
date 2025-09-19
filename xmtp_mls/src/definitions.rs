//! commonly used type definitions specific to mls
use std::sync::Arc;
use crate::context::XmtpMlsLocalContext;
use xmtp_api::ApiDebugWrapper;
use xmtp_api_d14n::queries::V3Client;
use xmtp_api_grpc::GrpcClient;

pub type MlsContext = Arc<
    XmtpMlsLocalContext<
        ApiDebugWrapper<V3Client<GrpcClient>>,
        xmtp_db::DefaultStore,
        xmtp_db::DefaultMlsStore,
    >,
>;
