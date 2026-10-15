//! Give the Apple cdylib a fixed install name. ld64 otherwise records the
//! output's absolute path, and a compiler-cache restore of the dylib would
//! name the checkout that first built it.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_VENDOR").as_deref() == Ok("apple") {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/libxmtp_sdk.dylib");
    }
}
