use std::process::Command;

fn main() {
    Command::new("make")
        .args(["libxmtp-version"])
        .status()
        .expect("failed to make libxmtp-version");
}
