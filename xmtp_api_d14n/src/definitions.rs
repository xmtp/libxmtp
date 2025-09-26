use xmtp_api_grpc::GrpcClient;

xmtp_common::if_v3! {
    pub type ApiClient = crate::V3Client<GrpcClient>;
}

//TODO:d14n ApiBuilder trait is trying to fit  a square peg in round hole with building apis for clients
//w/ more tha one grpc client. I.E what happens when more nodes?
//need a better abstraction for that builder
xmtp_common::if_d14n! {
    pub type ApiClient = crate::D14nClient<GrpcClient, GrpcClient>;
}

//TODO:d14n define combined client/feature flag
