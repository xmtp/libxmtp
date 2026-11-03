use xmtp_common::time::Duration;

#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(NeedsDbReconnect: Send + Sync))]
#[cfg_attr(target_arch = "wasm32", trait_variant::make(NeedsDbReconnect: xmtp_common::Wasm))]
pub trait LocalNeedsDbReconnect: std::error::Error {
    fn needs_db_reconnect(&self) -> bool;

    /// The least time to wait before restarting a worker that failed with
    /// this error: a rate-limit rejection's delay (RATE-041).
    fn restart_delay(&self) -> Option<Duration> {
        None
    }
}
