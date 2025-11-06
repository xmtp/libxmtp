//! type definitions for different backends (mock/v3/d14n)

use std::sync::Arc;

use xmtp_proto::api::mock::MockNetworkClient;

use crate::{
    protocol::{CursorStore, NoCursorStore},
    D14nClient, TrackedStatsClient, V3Client,
};

/// The Native/Wasm tonic gRPC client
pub type TestGrpcClient = xmtp_api_grpc::GrpcClient;
/// gRPC api error
pub type ApiError = xmtp_api_grpc::error::GrpcError;

/// test client that speaks only v3
pub type TestV3Client = TrackedStatsClient<V3Client<TestGrpcClient, Arc<dyn CursorStore>>>;
/// test client that speaks only d14n
pub type TestD14nClient =
    TrackedStatsClient<D14nClient<TestGrpcClient, TestGrpcClient, Arc<dyn CursorStore>>>;

/// V3 client with mock network
pub type MockV3Client = V3Client<MockNetworkClient, NoCursorStore>;

/// D14n client with mocked networks
pub type MockD14nClient = D14nClient<MockNetworkClient, MockNetworkClient, NoCursorStore>;

xmtp_common::if_d14n! {
    pub type MockClient = MockD14nClient;
    pub type TestClient = TestD14nClient;
}

xmtp_common::if_v3! {
    pub type MockClient = MockV3Client;
    pub type TestClient = TestV3Client;
}

// TODO: combined_migration client. i.e 'if_combined_client!'
