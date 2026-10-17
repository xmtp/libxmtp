/// Local chain address when no worktree environment is loaded.
pub const ANVIL_URL_DEFAULT: &str = "http://127.0.0.1:8545";

/// Local chain used for smart-contract-wallet verification.
pub struct DockerUrls;
impl DockerUrls {
    /// Anvil's address. Each worktree publishes it on its own port, so the
    /// value comes from `ANVIL_URL`, which `dev/worktree-env` writes.
    pub fn anvil() -> String {
        #[cfg(not(target_arch = "wasm32"))]
        {
            std::env::var("ANVIL_URL").unwrap_or_else(|_| ANVIL_URL_DEFAULT.to_string())
        }
        #[cfg(target_arch = "wasm32")]
        {
            option_env!("ANVIL_URL")
                .unwrap_or(ANVIL_URL_DEFAULT)
                .to_string()
        }
    }
}
