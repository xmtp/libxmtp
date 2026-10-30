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

/// The most chain calls one batch of smart contract wallet checks keeps in
/// flight, so a batch never starts every check before the first completes.
pub const MAX_CHAIN_CALLS_IN_FLIGHT: usize = 8;

/// The longest one chain RPC call may take, its permit wait included.
pub const CHAIN_CALL_TIMEOUT_MS: u64 = 10_000;

/// The most calls one chain provider has in flight, across all requests.
pub const MAX_CHAIN_CALLS_PER_PROVIDER: usize = 64;

/// The most ERC-6492 signatures one verify request carries: two for each
/// action of the largest identity update, so no update needs more. The
/// backend refuses a larger `max_scw_signatures`, and a client sizes its
/// requests by the smaller of this and the published value.
pub const MAX_SCW_SIGNATURES_PER_REQUEST: usize = 2 * super::MAX_IDENTITY_UPDATE_ACTIONS;

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
