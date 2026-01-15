#[xmtp_common::test]
async fn try_test() {
    println!("t");
}

#[xmtp_common::test]
fn try_test_sync() {
    println!("t");
}

#[xmtp_common::test(flavor = "multi_thread", worker_threads = 2)]
async fn try_test_flavor() {
    println!("t");
}

#[allow(clippy::unnecessary_literal_unwrap)]
#[xmtp_common::test(unwrap_try = true)]
#[should_panic]
async fn try_unwrap_try() {
    Err::<(), ()>(())?;
}

#[xmtp_common::test(disable_logging = true)]
fn try_disable_logging() {
    tracing::info!("t");
}
