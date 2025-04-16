use std::process::Command;

fn main() {
    Command::new("make")
        .args(["libxmtp-version"])
        .status()
        .expect("failed to make libxmtp-version");

    let output = Command::new("git")
        .args(&["rev-parse", "--short=7", "HEAD"])
        .output();
    
    let git_hash = match output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        },
        _ => "unknown".to_string()
    };
    
    println!("cargo:rustc-env=GIT_COMMIT_SHA={}", git_hash);
}
