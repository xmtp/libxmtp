use std::sync::LazyLock;
use tokio::sync;
use toxiproxy_rust::TOXIPROXY;

static TOXIPROXY_TEST_LOCK: LazyLock<sync::Mutex<()>> = LazyLock::new(|| sync::Mutex::new(()));

// TODO: can add this to the macro
pub async fn toxiproxy_test<T, F: AsyncFn() -> T>(f: F) -> T {
    let _g = TOXIPROXY_TEST_LOCK.lock().await;
    TOXIPROXY.reset().await.unwrap();
    f().await
}
