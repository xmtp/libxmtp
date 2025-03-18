#[xmtp_macro::test]
async fn try_test() {
    println!("t");
}

#[xmtp_macro::test]
fn try_test_sync() {
    println!("t");
}

#[xmtp_macro::test(flavor = "multi_thread")]
async fn try_test_flavor() {
    println!("t");
}
