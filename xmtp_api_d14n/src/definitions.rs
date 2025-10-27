use std::sync::Arc;

use xmtp_api_grpc::GrpcClient;

use crate::{
    AuthMiddleware, D14nClient, MultiNodeClient, V3Client,
    protocol::{CursorStore, NoCursorStore},
};

xmtp_common::if_v3! {
    pub type ApiClient = crate::V3Client<GrpcClient, NoCursorStore>;
}

xmtp_common::if_d14n! {
    pub type ApiClient = crate::D14nClient<GrpcClient, GrpcClient, NoCursorStore>;
}

pub type FullD14nClient = D14nClient<MultiNodeClient, GrpcClient, Arc<dyn CursorStore>>;
pub type FullD14nAuthClient =
    D14nClient<MultiNodeClient, AuthMiddleware<GrpcClient>, Arc<dyn CursorStore>>;
pub type FullV3Client = V3Client<GrpcClient, Arc<dyn CursorStore>>;
