//! Backend connection values.

pub const LOCALHOST: &str = "http://localhost";
/// Maximum bytes in one gRPC payload.
pub const GRPC_PAYLOAD_LIMIT: usize = 1024 * 1024 * 25;

/// Local backend URL when no worktree environment is loaded.
pub const BACKEND_TEST_URL_DEFAULT: &str = "http://localhost:5050";
/// Local backend proxy URL when no worktree environment is loaded.
pub const BACKEND_TEST_TOXIC_URL_DEFAULT: &str = "http://localhost:6010";

/// Local backend URL for client tests.
///
/// Each worktree publishes the stack on its own ports, so the address comes
/// from the environment. `dev/worktree-env` writes it and the `just` recipes
/// export it. The constant above is the fallback for a bare `cargo test`.
///
/// Native reads the variable at run time. Wasm has no `std::env`, so the value
/// is baked in at build time instead; build through `just wasm test` so the
/// worktree's environment is loaded.
pub fn backend_test_url() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::var("XMTP_BACKEND_URL").unwrap_or_else(|_| BACKEND_TEST_URL_DEFAULT.to_string())
    }
    #[cfg(target_arch = "wasm32")]
    {
        option_env!("XMTP_BACKEND_URL")
            .unwrap_or(BACKEND_TEST_URL_DEFAULT)
            .to_string()
    }
}

/// Local backend proxy URL for fault tests. See [`backend_test_url`].
pub fn backend_test_toxic_url() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::var("XMTP_BACKEND_TOXIC_URL")
            .unwrap_or_else(|_| BACKEND_TEST_TOXIC_URL_DEFAULT.to_string())
    }
    #[cfg(target_arch = "wasm32")]
    {
        option_env!("XMTP_BACKEND_TOXIC_URL")
            .unwrap_or(BACKEND_TEST_TOXIC_URL_DEFAULT)
            .to_string()
    }
}

/// Temporary test URL names. Remove with the Task 9 binding update.
pub struct GrpcUrls;
impl GrpcUrls {
    pub fn node() -> String {
        backend_test_url()
    }
    pub fn gateway() -> String {
        backend_test_url()
    }
}

/// Temporary fault-test URL name. Remove with the Task 9 binding update.
pub struct GrpcUrlsToxic;
impl GrpcUrlsToxic {
    pub fn node() -> String {
        backend_test_toxic_url()
    }
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
