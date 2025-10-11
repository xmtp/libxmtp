use xmtp_api_grpc::GrpcClient;

xmtp_common::if_v3! {
    pub type ApiClient = crate::V3Client<GrpcClient>;
}

xmtp_common::if_d14n! {
    pub type ApiClient = crate::D14nClient<GrpcClient, GrpcClient>;
}
