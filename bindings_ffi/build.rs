use std::process::Command;

fn main() {
    uniffi::generate_scaffolding("./src/xmtpv3.udl").expect("Building the UDL file failed");
    Command::new("make")
        .args(["libxmtp-version"])
        .status()
        .expect("failed to make libxmtp-version");
}
