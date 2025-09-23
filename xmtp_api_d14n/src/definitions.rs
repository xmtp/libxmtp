use xmtp_api_grpc::GrpcClient;
use xmtp_proto::prelude::XmtpMlsStreams;

use crate::{D14nClient, V3Client};

xmtp_common::if_v3! {
    pub type ApiClient = crate::V3Client<GrpcClient>;
}

//TODO:d14n ApiBuilder trait is trying to fit  a square peg in round hole with building apis for clients
//w/ more tha one grpc client. I.E what happens when more nodes?
//need a better abstraction for that builder
xmtp_common::if_d14n! {
    pub type ApiClient = crate::D14nClient<GrpcClient, GrpcClient>;
}

pub type D14nGroupStream =
    <D14nClient<GrpcClient, GrpcClient> as XmtpMlsStreams>::GroupMessageStream;
pub type D14nWelcomeStream =
    <D14nClient<GrpcClient, GrpcClient> as XmtpMlsStreams>::WelcomeMessageStream;

pub type V3GroupStream = <V3Client<GrpcClient> as XmtpMlsStreams>::GroupMessageStream;
pub type V3WelcomeStream = <V3Client<GrpcClient> as XmtpMlsStreams>::WelcomeMessageStream;

//TODO:d14n define combined client/feature flag
