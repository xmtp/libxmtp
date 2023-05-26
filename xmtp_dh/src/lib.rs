uniffi_macros::include_scaffolding!("xmtp_dh");

pub use xmtp_dh;

pub fn e2e_selftest() -> String {
    // Returns Result<String>
    xmtpv3::manager::e2e_selftest()
        .map_err(|e| format!("{:?}", e))
        .unwrap()
}
