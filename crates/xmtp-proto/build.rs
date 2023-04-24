use std::error::Error;
use std::process::{exit, Command, Stdio};

fn ensure_crate(crate_name: &str) {
    let check_crate = Command::new("cargo")
        .arg("install")
        .arg("--list")
        .output()
        .expect("Failed to list installed crates");

    let installed_crates = String::from_utf8_lossy(&check_crate.stdout);
    if !installed_crates.contains(crate_name) {
        let mut install_crate = Command::new("cargo")
            .arg("install")
            .arg(crate_name)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("Failed to install crate");

        install_crate
            .wait()
            .expect("Failed to wait on crate installation process");
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    ensure_crate("protoc-gen-prost-crate");

    let status = Command::new("buf")
        .arg("generate")
        .arg("https://github.com/xmtp/proto.git#branch=michaelx-rust_v3_protos,subdir=proto")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .unwrap();

    if !status.success() {
        exit(status.code().unwrap_or(-1))
    }

    Ok(())
}
