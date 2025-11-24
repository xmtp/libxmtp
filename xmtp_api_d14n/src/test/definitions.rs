//! type definitions for different backends (mock/v3/d14n)
//! "creators" are test clients that can be used with [`XmtpTestClient`]

use crate::{
    D14nClient, ReadWriteClient, ReadonlyClient, TrackedStatsClient, V3Client,
    protocol::{CursorStore, NoCursorStore},
};
use std::sync::Arc;
use xmtp_api_grpc::{
    GrpcClient,
    test::{
        DevGatewayClient, DevXmtpdClient, GatewayClient, LocalGatewayClient, LocalNodeGoClient,
        LocalXmtpdClient, NodeGoClient, ToxicGatewayClient, ToxicNodeGoClient, ToxicXmtpdClient,
        XmtpdClient,
    },
};
use xmtp_proto::api::mock::MockNetworkClient;

/// gRPC api error
pub type ApiError = xmtp_api_grpc::error::GrpcError;

/// test client that speaks only v3
/// switches local/dev on feature flag
pub type TestV3Client = TrackedStatsClient<V3Client<GrpcClient, Arc<dyn CursorStore>>>;

/// A built test client that speaks only d14n.
/// switches local/dev on feature flag
pub type TestD14nClient =
    TrackedStatsClient<D14nClient<ReadWriteClient<GrpcClient, GrpcClient>, Arc<dyn CursorStore>>>;

/// Creator for a feature-flag switchable D14n client
pub type TestD14nClientCreator = TrackedStatsClient<
    D14nClient<ReadWriteClient<XmtpdClient, GatewayClient>, Arc<dyn CursorStore>>,
>;
/// Creator for a feature-flag switchable V3 client
pub type TestV3ClientCreator = TrackedStatsClient<V3Client<NodeGoClient, Arc<dyn CursorStore>>>;

/// A that only communicates with dev docker
/// _does not switch on feature flag_
pub type DevOnlyD14nClientCreator = TrackedStatsClient<
    D14nClient<ReadWriteClient<DevXmtpdClient, DevGatewayClient>, Arc<dyn CursorStore>>,
>;

/// A client that only communicates with dev network
/// _does not switch on feature flag_
pub type DevOnlyV3ClientCreator = TrackedStatsClient<V3Client<NodeGoClient, Arc<dyn CursorStore>>>;

/// A client that only communicates with local docker
/// _does not switch on feature flag_
pub type LocalOnlyD14nClientCreator = TrackedStatsClient<
    D14nClient<ReadWriteClient<LocalXmtpdClient, LocalGatewayClient>, Arc<dyn CursorStore>>,
>;

/// client that only communicates with local docker
/// _does not switch on feature flag_
pub type LocalOnlyV3ClientCreator =
    TrackedStatsClient<V3Client<LocalNodeGoClient, Arc<dyn CursorStore>>>;

/// A client that only communicates with local docker toxiproxy
/// _does not switch on feature flag_
pub type ToxicOnlyD14nClientCreator = TrackedStatsClient<
    D14nClient<ReadWriteClient<ToxicXmtpdClient, ToxicGatewayClient>, Arc<dyn CursorStore>>,
>;

/// client that only communicates with local docker toxiproxy
/// _does not switch on feature flag_
pub type ToxicOnlyV3ClientCreator =
    TrackedStatsClient<V3Client<ToxicNodeGoClient, Arc<dyn CursorStore>>>;

/// A client that only reads from local docker
/// _does not switch on feature flag_
pub type ReadOnlyD14nClientCreator = TrackedStatsClient<
    D14nClient<
        ReadonlyClient<ReadWriteClient<LocalXmtpdClient, LocalGatewayClient>>,
        Arc<dyn CursorStore>,
    >,
>;
/// A client that only reads from local docker
/// _does not switch on feature flag_
pub type ReadOnlyV3ClientCreator =
    TrackedStatsClient<V3Client<ReadonlyClient<LocalNodeGoClient>, Arc<dyn CursorStore>>>;

/// V3 client with mock network
pub type MockV3Client = V3Client<MockNetworkClient, NoCursorStore>;
/// D14n client with mocked networks
pub type MockD14nClient = D14nClient<MockNetworkClient, NoCursorStore>;

xmtp_common::if_d14n! {
    pub type MockClient = MockD14nClient;
    pub type TestClient = TestD14nClient;
    /// Test client that is local only, but still switches between d14n/v3 clients on feature flag
    pub type LocalOnlyTestClientCreator = LocalOnlyD14nClientCreator;
    /// Test client that is dev only, but still switches between d14n/v3 clients on feature flag
    pub type DevOnlyTestClientCreator = DevOnlyD14nClientCreator;
    /// Test client that connects to toxiproxy only, but still switches between d14n/v3 clients on
    /// feature flag
    pub type ToxicOnlyTestClientCreator = ToxicOnlyD14nClientCreator;
    /// Test client that is local-only and read-only, but still switches between d14n/v3 clients on feature flag
    pub type ReadOnlyTestClientCreator = ReadOnlyD14nClientCreator;
    /// Client builder that builds a client which communicates with local/dev v3/d14n based on
    /// feature flag
    pub type FeatureSwitchedTestClientCreator = TestD14nClientCreator;
    pub type DefaultTestClientCreator = FeatureSwitchedTestClientCreator;
}

xmtp_common::if_v3! {
    pub type MockClient = MockV3Client;
    pub type TestClient = TestV3Client;
    /// Test client that is local only, but still switches between d14n/v3 clients on feature flag
    pub type LocalOnlyTestClientCreator = LocalOnlyV3ClientCreator;
    /// Test client that is dev only, but still switches between d14n/v3 clients on feature flag
    pub type DevOnlyTestClientCreator = DevOnlyV3ClientCreator;
    /// Test client that connects to toxiproxy only, but still switches between d14n/v3 clients on
    /// feature flag
    pub type ToxicOnlyTestClientCreator = ToxicOnlyV3ClientCreator;
    /// Test client that is local-only and read-only, but still switches between d14n/v3 clients on feature flag
    pub type ReadOnlyTestClientCreator = ReadOnlyV3ClientCreator;
    /// Client builder that builds a client which communicates with local/dev v3/d14n based on
    /// feature flag
    pub type FeatureSwitchedTestClientCreator = TestV3ClientCreator;
    pub type DefaultTestClientCreator = FeatureSwitchedTestClientCreator;
}

// TODO: combined_migration client. i.e 'if_combined_client!'
