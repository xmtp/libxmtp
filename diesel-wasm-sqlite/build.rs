use std::env;
use std::process::Command;

fn main() {
    // Tell Cargo that if the given file changes, to rerun this build script.
    println!("cargo::rerun-if-changed=package.js");
    // println!("cargo::rerun-if-changed=package.js");
    // can run yarn run build here too
    // let out_dir = env::var("OUT_DIR").unwrap();

    Command::new("yarn")
        .args(["run", "build"])
        .status()
        .unwrap();
    /*
        Command::new("cp")
            .args(["src/wa-sqlite.wasm"])
            .arg(&format!("{}/wa-sqlite.wasm", out_dir))
            .status()
            .unwrap();
    */
}
