fn main() {
    println!("cargo:rerun-if-env-changed=XMTP_TEST_LOGGING");
    println!("cargo:rerun-if-env-changed=CI");
}
