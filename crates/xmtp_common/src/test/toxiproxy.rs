use std::sync::LazyLock;
use tokio::sync;
use toxiproxy_rust::client::Client;

static TOXIPROXY_TEST_LOCK: LazyLock<sync::Mutex<()>> = LazyLock::new(|| sync::Mutex::new(()));

/// Toxiproxy's control address when no worktree environment is loaded.
const TOXIPROXY_API_ADDR_DEFAULT: &str = "127.0.0.1:8474";

/// Toxiproxy's control address for this worktree, as `host:port`.
///
/// This lives here rather than in `xmtp_configuration` because that crate
/// depends on this one. `dev/worktree-env` writes the variable as a URL for
/// consistency with the other addresses, so trim it to a socket address.
fn api_addr() -> String {
    std::env::var("XMTP_TOXIPROXY_API")
        .map(|url| {
            url.trim_start_matches("http://")
                .trim_start_matches("https://")
                .trim_end_matches('/')
                .to_string()
        })
        .unwrap_or_else(|_| TOXIPROXY_API_ADDR_DEFAULT.to_string())
}

/// The Toxiproxy control client for this worktree.
///
/// `toxiproxy_rust::TOXIPROXY` hardcodes `127.0.0.1:8474`, but each worktree
/// publishes Toxiproxy on its own port, so build the client from this
/// worktree's address instead of using that static.
static TOXIPROXY: LazyLock<Client> = LazyLock::new(|| Client::new(api_addr()));

pub fn toxiproxy() -> &'static Client {
    &TOXIPROXY
}

// TODO: can add this to the macro
pub async fn toxiproxy_test<T, F: AsyncFn() -> T>(f: F) -> T {
    let _g = TOXIPROXY_TEST_LOCK.lock().await;
    TOXIPROXY.reset().await.unwrap();
    f().await
}
