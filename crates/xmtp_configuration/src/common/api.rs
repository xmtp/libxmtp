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

/// Maximum consecutive credential rejections and callback failures.
///
/// This value must stay above the failures one caller request can produce, or
/// a single API call locks the client out. `Retry::default()` makes 6 attempts
/// (5 retries), and the auth middleware replays each attempt once with a fresh
/// credential, so one call can count up to 12 failures. The limit is above
/// that, so only repeated calls reach the lockout. Raise it together with the
/// retry budget; `retry_budget_cannot_reach_the_auth_lockout` checks the
/// relation.
pub const MAX_CONSECUTIVE_AUTH_FAILURES: u32 = 13;
/// Time before one authentication probe is allowed after lockout.
pub const AUTH_LOCKOUT_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(60);
