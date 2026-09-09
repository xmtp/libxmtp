//! Backend connection values.

pub const LOCALHOST: &str = "http://localhost";
/// Maximum bytes in one gRPC payload.
pub const GRPC_PAYLOAD_LIMIT: usize = 1024 * 1024 * 25;
/// Local backend URL for client tests.
pub const BACKEND_TEST_URL: &str = "http://localhost:5050";
/// Local backend proxy URL for fault tests.
pub const BACKEND_TEST_TOXIC_URL: &str = "http://localhost:6010";

/// Temporary test URL names. Remove with the Task 9 binding update.
pub struct GrpcUrls;
impl GrpcUrls {
    pub const NODE: &'static str = BACKEND_TEST_URL;
    pub const GATEWAY: &'static str = BACKEND_TEST_URL;
}

/// Temporary fault-test URL name. Remove with the Task 9 binding update.
pub struct GrpcUrlsToxic;
impl GrpcUrlsToxic {
    pub const NODE: &'static str = BACKEND_TEST_TOXIC_URL;
}
