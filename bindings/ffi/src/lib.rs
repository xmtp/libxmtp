uniffi_macros::include_scaffolding!("xmtpv3");

pub use xmtpv3;

pub fn e2e_selftest() -> String {
    // Returns Result<String>
    xmtpv3::e2e_selftest()
        .map_err(|e| format!("{:?}", e))
        .unwrap()
}
