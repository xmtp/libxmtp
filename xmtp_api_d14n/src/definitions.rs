use xmtp_api_grpc::GrpcClient;

use crate::protocol::NoCursorStore;

xmtp_common::if_v3! {
    pub type ApiClient = crate::V3Client<GrpcClient, NoCursorStore>;
}

xmtp_common::if_d14n! {
    pub type ApiClient = crate::D14nClient<GrpcClient, GrpcClient, NoCursorStore>;
}
