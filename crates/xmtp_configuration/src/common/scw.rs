/// Local chain address when no worktree environment is loaded.
pub const ANVIL_URL_DEFAULT: &str = "http://127.0.0.1:8545";

/// The longest ERC-6492 signature a validator verifies: the verifier's
/// ceiling.
///
/// Deployless verification sends initcode, which EIP-3860 limits to 49,152
/// bytes: the 1,792 bytes of the universal signature validator, then its
/// ABI-encoded constructor arguments, a 128-byte head (the signer, the hash,
/// the signature's offset and its length) and the signature padded to a
/// multiple of 32 bytes. 49,152 - 1,792 - 128 = 47,232, itself a multiple of
/// 32, so a signature one byte longer can never be verified this way.
pub const MAX_SCW_SIGNATURE_BYTES: usize = 47_232;

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
